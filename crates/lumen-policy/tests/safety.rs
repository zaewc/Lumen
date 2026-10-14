//! Safety properties of the policy (roadmap 7.8, ADR-0014 "Testing").
//!
//! Random inputs mix every fact, coverage state, capability, path and Jev
//! judgment the policy reads; the properties must hold for all of them.

use std::collections::{BTreeMap, BTreeSet};
use std::num::NonZeroU32;

use lumen_domain::{
    AccessState, AppDataKind, AppId, AppIdScheme, Assessment, Basis, ByteCount, CleanupMechanism,
    EntryKind, EntryTimes, Evidence, EvidenceHash, Fact, FileId, FileIdentity, FilesystemEntry,
    ItemRef, JevEffect, Judgment, JudgmentConfidence, Platform, PolicyDecision, PolicyStage,
    Protection, Provenance, RawPath, ReasonCode, ReclaimEstimate, SizeFacts, SizeFlags, SourceName,
    Subject, Timestamp, UntrustedText, Verdict, VolumeId,
};
use lumen_policy::{Context, KnowledgeClass, Observed, PolicyInput, evaluate};
use proptest::prelude::*;

fn subject() -> Subject {
    Subject::File {
        identity: FileIdentity {
            volume: VolumeId::new("v").unwrap_or_else(|_| unreachable!()),
            file: FileId::new(1),
        },
    }
}

fn evidence(fact: Fact) -> Evidence {
    Evidence {
        subject: subject(),
        fact,
        basis: Basis::Observed,
        provenance: Provenance {
            source: SourceName::new("prop").unwrap_or_else(|_| unreachable!()),
            observed_at: Timestamp::UNIX_EPOCH,
        },
    }
}

fn entry_name(n: u8) -> SourceName {
    SourceName::new(format!("entry-{n}")).unwrap_or_else(|_| unreachable!())
}

fn fact() -> impl Strategy<Value = Fact> {
    prop_oneof![
        any::<bool>().prop_map(Fact::InUse),
        any::<bool>().prop_map(Fact::OwnedByCurrentUser),
        proptest::option::of(0_u32..5)
            .prop_map(|unseen| Fact::NotInUseByVisibleProcesses { unseen }),
        any::<bool>().prop_map(Fact::CloudBacked),
        (any::<bool>(), any::<bool>(), any::<bool>(), any::<bool>()).prop_map(
            |(system_restricted, undeletable, system_attribute, hidden)| {
                Fact::Protected(Protection {
                    system_restricted,
                    undeletable,
                    system_attribute,
                    hidden,
                })
            }
        ),
        (0_u8..4).prop_map(|n| Fact::KnowledgeBaseMatch {
            entry: entry_name(n)
        }),
        Just(Fact::VendorDeclaredCache {
            declared_by: UntrustedText::new("vendor"),
        }),
        Just(Fact::OwningApplication(AppId {
            scheme: AppIdScheme::BundleId,
            value: UntrustedText::new("com.example.app"),
        })),
        prop_oneof![
            Just(AppDataKind::Caches),
            Just(AppDataKind::Container),
            Just(AppDataKind::Preferences),
        ]
        .prop_map(|kind| Fact::AppDataLocation { kind }),
    ]
}

fn class() -> impl Strategy<Value = KnowledgeClass> {
    prop_oneof![
        Just(KnowledgeClass::SelfManaged),
        Just(KnowledgeClass::ToolAction),
        Just(KnowledgeClass::Regenerable),
        Just(KnowledgeClass::NotRegenerable),
    ]
}

fn kind() -> impl Strategy<Value = EntryKind> {
    prop_oneof![
        4 => Just(EntryKind::File),
        4 => Just(EntryKind::Directory),
        1 => Just(EntryKind::Symlink),
        1 => Just(EntryKind::OtherReparsePoint),
    ]
}

fn observed() -> impl Strategy<Value = Option<Observed>> {
    proptest::option::weighted(
        0.9,
        (kind(), any::<bool>(), 0_u64..3, any::<bool>()).prop_map(
            |(kind, complete, reclaimable, readable)| Observed {
                entry: FilesystemEntry {
                    path: RawPath::from_unix_bytes(b"/home/t/x".to_vec())
                        .unwrap_or_else(|_| unreachable!()),
                    kind,
                    size: SizeFacts {
                        identity: FileIdentity {
                            volume: VolumeId::new("v").unwrap_or_else(|_| unreachable!()),
                            file: FileId::new(1),
                        },
                        logical: ByteCount::new(reclaimable),
                        allocated: ByteCount::new(reclaimable),
                        private: None,
                        clone_id: None,
                        link_count: NonZeroU32::MIN,
                        flags: SizeFlags::default(),
                    },
                    times: EntryTimes::default(),
                    protection: Protection::default(),
                    access: if readable {
                        AccessState::Readable
                    } else {
                        AccessState::NotScanned
                    },
                },
                complete,
                reclaim: ReclaimEstimate {
                    reclaimable_now: ByteCount::new(reclaimable),
                    ..ReclaimEstimate::default()
                },
            },
        ),
    )
}

fn path() -> impl Strategy<Value = Vec<RawPath>> {
    proptest::collection::vec(
        prop_oneof![
            Just("/home/t/.cache/x"),
            Just("/home/t/.ssh/id_ed25519"),
            Just("/usr/lib/x"),
            Just("/usr/local/x"),
            Just("/System/Library/x"),
        ]
        .prop_map(|p| {
            RawPath::from_unix_bytes(p.as_bytes().to_vec()).unwrap_or_else(|_| unreachable!())
        }),
        0..3,
    )
}

fn judgment() -> impl Strategy<Value = Option<Judgment>> {
    proptest::option::of(
        (
            prop_oneof![
                Just(Assessment::LikelyRemovable),
                Just(Assessment::LikelyNeeded),
                Just(Assessment::Uncertain),
            ],
            any::<bool>(),
        )
            .prop_map(|(assessment, injection_suspected)| Judgment {
                item_ref: ItemRef::new("item").unwrap_or_else(|_| unreachable!()),
                assessment,
                confidence: JudgmentConfidence::High,
                reason_codes: BTreeSet::from([ReasonCode::RegeneratableCache]),
                evidence_refs: BTreeSet::new(),
                injection_suspected,
                rationale: UntrustedText::new("ignored"),
            }),
    )
}

prop_compose! {
    fn policy_input()(
        facts in proptest::collection::vec(fact(), 0..8),
        classes in proptest::collection::btree_map(0_u8..4, class(), 0..4),
        observed in observed(),
        paths in path(),
        platform in prop_oneof![Just(Platform::Macos), Just(Platform::Linux)],
        quarantine in any::<bool>(),
        jev in judgment(),
    ) -> PolicyInput {
        PolicyInput {
            subject: subject(),
            evidence_hash: EvidenceHash::from_bytes([3; 32]),
            evidence: facts.into_iter().map(evidence).collect(),
            paths,
            context: Context {
                platform,
                home: Some(
                    RawPath::from_unix_bytes(b"/home/t".to_vec())
                        .unwrap_or_else(|_| unreachable!()),
                ),
                windows_dir: None,
                local_app_data: None,
                mechanisms: if quarantine {
                    BTreeSet::from([CleanupMechanism::QuarantineRename])
                } else {
                    BTreeSet::new()
                },
                complete_sources: BTreeSet::new(),
            },
            observed,
            knowledge: classes
                .into_iter()
                .map(|(n, c)| (entry_name(n), c))
                .collect::<BTreeMap<_, _>>(),
            jev,
        }
    }
}

fn decide(input: &PolicyInput) -> Result<PolicyDecision, TestCaseError> {
    evaluate(input).map_err(|e| TestCaseError::fail(format!("evaluate failed: {e}")))
}

/// Whether the input carries a fact or state that must keep the item.
fn must_keep(input: &PolicyInput) -> bool {
    let facts: Vec<&Fact> = input.evidence.iter().map(|e| &e.fact).collect();
    let protected = facts.iter().any(|f| match f {
        Fact::Protected(p) => p.os_protected() || p.system_attribute,
        Fact::CloudBacked(true) | Fact::InUse(true) => true,
        _ => false,
    });
    let complete_no = facts.contains(&&Fact::InUse(false));
    let partial = facts
        .iter()
        .any(|f| matches!(f, Fact::NotInUseByVisibleProcesses { .. }));
    // ADR-0030: a partial answer passes only for the user's own items below
    // their home, when the reversible quarantine rename is available.
    let adr_0030 = facts.contains(&&Fact::OwnedByCurrentUser(true))
        && !facts.contains(&&Fact::OwnedByCurrentUser(false))
        && !input.paths.is_empty()
        && input
            .paths
            .iter()
            .all(|p| p.as_bytes().starts_with(b"/home/t/"))
        && input
            .context
            .mechanisms
            .contains(&CleanupMechanism::QuarantineRename);
    let in_use_gate_passes = complete_no || (partial && adr_0030);
    let incomplete = input
        .observed
        .as_ref()
        .is_none_or(|o| !o.complete || !o.entry.access.is_observed());
    let system_path = input.context.platform == Platform::Macos
        && input.paths.iter().any(|p| {
            let bytes = p.as_bytes();
            bytes.starts_with(b"/System/")
                || (bytes.starts_with(b"/usr/") && !bytes.starts_with(b"/usr/local/"))
        });
    let sensitive_path = input
        .paths
        .iter()
        .any(|p| p.as_bytes().starts_with(b"/home/t/.ssh/"));
    protected || !in_use_gate_passes || incomplete || system_path || sensitive_path
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2048))]

    #[test]
    fn protected_items_are_always_kept(input in policy_input()) {
        let decision = decide(&input)?;
        if must_keep(&input) {
            prop_assert_eq!(decision.verdict(), Verdict::Keep);
            prop_assert_eq!(decision.jev_effect(), JevEffect::None);
        }
    }

    #[test]
    fn identical_inputs_in_any_order_decide_identically(input in policy_input()) {
        let mut reversed = input.clone();
        reversed.evidence.reverse();
        prop_assert_eq!(decide(&input)?, decide(&reversed)?);
    }

    #[test]
    fn jev_never_lowers_caution(input in policy_input()) {
        let mut without = input.clone();
        without.jev = None;
        let with_jev = decide(&input)?.verdict();
        let without_jev = decide(&without)?.verdict();
        // `Keep` < `Review` < `Quarantine`: smaller is more cautious.
        prop_assert!(with_jev <= without_jev, "{:?} vs {:?}", with_jev, without_jev);
    }

    #[test]
    fn quarantine_needs_a_regenerable_classification_and_no_cap(input in policy_input()) {
        let decision = decide(&input)?;
        if decision.verdict() == Verdict::Quarantine {
            let rules: Vec<&str> = decision.fired_rules().iter().map(|r| r.rule.as_str()).collect();
            prop_assert!(
                rules.contains(&"classify.devkb-regenerable") || rules.contains(&"classify.app-cache"),
                "{:?}", rules
            );
            prop_assert!(
                decision.fired_rules().iter().all(|r| r.stage != PolicyStage::Eligibility
                    && r.stage != PolicyStage::HardProtection),
                "{:?}", rules
            );
            prop_assert!(input.context.mechanisms.contains(&CleanupMechanism::QuarantineRename));
        }
    }
}

/// The properties above are not vacuous: a free, fully read, regenerable item
/// on a platform that can quarantine is a quarantine candidate.
#[test]
fn quarantine_is_reachable() -> Result<(), lumen_policy::PolicyError> {
    let input = PolicyInput {
        subject: subject(),
        evidence_hash: EvidenceHash::from_bytes([3; 32]),
        evidence: vec![
            evidence(Fact::InUse(false)),
            evidence(Fact::KnowledgeBaseMatch {
                entry: entry_name(0),
            }),
        ],
        paths: Vec::new(),
        context: Context {
            platform: Platform::Linux,
            home: Some(
                RawPath::from_unix_bytes(b"/home/t".to_vec()).unwrap_or_else(|_| unreachable!()),
            ),
            windows_dir: None,
            local_app_data: None,
            mechanisms: BTreeSet::from([CleanupMechanism::QuarantineRename]),
            complete_sources: BTreeSet::new(),
        },
        observed: Some(Observed {
            entry: FilesystemEntry {
                path: RawPath::from_unix_bytes(b"/home/t/x".to_vec())
                    .unwrap_or_else(|_| unreachable!()),
                kind: EntryKind::Directory,
                size: SizeFacts {
                    identity: FileIdentity {
                        volume: VolumeId::new("v").unwrap_or_else(|_| unreachable!()),
                        file: FileId::new(1),
                    },
                    logical: ByteCount::new(1),
                    allocated: ByteCount::new(1),
                    private: None,
                    clone_id: None,
                    link_count: NonZeroU32::MIN,
                    flags: SizeFlags::default(),
                },
                times: EntryTimes::default(),
                protection: Protection::default(),
                access: AccessState::Readable,
            },
            complete: true,
            reclaim: ReclaimEstimate {
                reclaimable_now: ByteCount::new(1),
                ..ReclaimEstimate::default()
            },
        }),
        knowledge: BTreeMap::from([(entry_name(0), KnowledgeClass::Regenerable)]),
        jev: None,
    };
    assert!(!must_keep(&input));
    assert_eq!(evaluate(&input)?.verdict(), Verdict::Quarantine);
    Ok(())
}
