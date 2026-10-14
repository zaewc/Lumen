//! The policy's rules, in evaluation order within each stage.
//!
//! Rules read only evidence about the subject itself; facts about neighbours
//! reach a decision through the relationships that cite them.

use std::collections::BTreeSet;

use lumen_domain::{
    ByteCount, CleanupMechanism, EntryKind, Evidence, EvidenceId, Fact, PolicyStage, RiskFactor,
    Subject, Verdict,
};

use crate::{Finding, PolicyInput, Rule};

/// Every rule the policy evaluates.
pub(crate) const RULES: &[Rule] = &[
    Rule {
        id: "protect.location",
        stage: PolicyStage::HardProtection,
        check: crate::locations::check,
    },
    Rule {
        id: "protect.os-protected",
        stage: PolicyStage::HardProtection,
        check: os_protected,
    },
    Rule {
        id: "protect.system-file",
        stage: PolicyStage::HardProtection,
        check: system_file,
    },
    Rule {
        id: "protect.cloud-backed",
        stage: PolicyStage::HardProtection,
        check: cloud_backed,
    },
    Rule {
        id: "protect.in-use",
        stage: PolicyStage::HardProtection,
        check: in_use,
    },
    Rule {
        id: "protect.in-use-partial",
        stage: PolicyStage::HardProtection,
        check: in_use_partial,
    },
    Rule {
        id: "protect.in-use-unknown",
        stage: PolicyStage::HardProtection,
        check: in_use_unknown,
    },
    Rule {
        id: "protect.not-observed",
        stage: PolicyStage::HardProtection,
        check: not_observed,
    },
    Rule {
        id: "protect.incomplete-scan",
        stage: PolicyStage::HardProtection,
        check: incomplete_scan,
    },
    Rule {
        id: "eligible.in-use-partial",
        stage: PolicyStage::Eligibility,
        check: in_use_partial_allowed,
    },
    Rule {
        id: "eligible.quarantine-unsupported",
        stage: PolicyStage::Eligibility,
        check: quarantine_unsupported,
    },
    Rule {
        id: "eligible.frees-nothing",
        stage: PolicyStage::Eligibility,
        check: frees_nothing,
    },
    Rule {
        id: "eligible.object-kind",
        stage: PolicyStage::Eligibility,
        check: object_kind,
    },
    Rule {
        id: "classify.devkb-regenerable",
        stage: PolicyStage::Classification,
        check: crate::classify::devkb_regenerable,
    },
    Rule {
        id: "classify.devkb-tool-action",
        stage: PolicyStage::Classification,
        check: crate::classify::devkb_tool_action,
    },
    Rule {
        id: "classify.devkb-not-regenerable",
        stage: PolicyStage::Classification,
        check: crate::classify::devkb_not_regenerable,
    },
    Rule {
        id: "classify.devkb-self-managed",
        stage: PolicyStage::Classification,
        check: crate::classify::devkb_self_managed,
    },
    Rule {
        id: "classify.devkb-unknown",
        stage: PolicyStage::Classification,
        check: crate::classify::devkb_unknown,
    },
    Rule {
        id: "classify.vendor-declared-cache",
        stage: PolicyStage::Classification,
        check: crate::classify::vendor_declared_cache,
    },
    Rule {
        id: "classify.app-cache",
        stage: PolicyStage::Classification,
        check: crate::classify::app_cache,
    },
    Rule {
        id: "classify.app-state",
        stage: PolicyStage::Classification,
        check: crate::classify::app_state,
    },
    Rule {
        id: "classify.orphan",
        stage: PolicyStage::Classification,
        check: crate::classify::orphan,
    },
];

/// Evidence about the subject itself whose fact satisfies `test`.
fn about(input: &PolicyInput, test: impl Fn(&Fact) -> bool) -> BTreeSet<EvidenceId> {
    input
        .evidence
        .iter()
        .filter(|e| e.subject == input.subject && test(&e.fact))
        .map(Evidence::id)
        .collect()
}

/// A protection citing `evidence`, if there is any.
fn keep_if(evidence: BTreeSet<EvidenceId>, risk: RiskFactor) -> Option<Finding> {
    (!evidence.is_empty()).then(|| Finding {
        verdict: Verdict::Keep,
        evidence,
        risk: BTreeSet::from([risk]),
        rule: None,
    })
}

/// The operating system forbids removal (SIP / `SF_RESTRICTED`, `SF_NOUNLINK`,
/// immutable flags).
fn os_protected(input: &PolicyInput) -> Option<Finding> {
    keep_if(
        about(
            input,
            |f| matches!(f, Fact::Protected(p) if p.os_protected()),
        ),
        RiskFactor::OsProtected,
    )
}

/// Windows marks the file as part of the operating system
/// (`FILE_ATTRIBUTE_SYSTEM`).
fn system_file(input: &PolicyInput) -> Option<Finding> {
    keep_if(
        about(
            input,
            |f| matches!(f, Fact::Protected(p) if p.system_attribute),
        ),
        RiskFactor::OsProtected,
    )
}

/// Cloud placeholders and synced items: removal may propagate to every device.
fn cloud_backed(input: &PolicyInput) -> Option<Finding> {
    keep_if(
        about(input, |f| *f == Fact::CloudBacked(true)),
        RiskFactor::CloudBacked,
    )
}

/// A process uses the item.
fn in_use(input: &PolicyInput) -> Option<Finding> {
    keep_if(about(input, |f| *f == Fact::InUse(true)), RiskFactor::InUse)
}

fn complete_no(input: &PolicyInput) -> bool {
    !about(input, |f| *f == Fact::InUse(false)).is_empty()
}

/// A partial answer is the one the in-use gate relies on: no process says
/// yes, no complete answer says no, but visible processes were checked.
fn partial_answer(input: &PolicyInput) -> BTreeSet<EvidenceId> {
    if complete_no(input) || in_use(input).is_some() {
        return BTreeSet::new();
    }
    about(input, |f| {
        matches!(f, Fact::NotInUseByVisibleProcesses { .. })
    })
}

/// ADR-0030 conditions 1 and 2 for passing on a partial answer: every path is
/// below the user's home, the user owns the item (and nothing says otherwise),
/// and the only action is the reversible quarantine rename. Condition 3 (no
/// other protection) is the evaluator's; condition 4 (re-probe before the
/// rename) is the executor's. Returns the ownership evidence relied on.
fn partial_allowed(input: &PolicyInput) -> BTreeSet<EvidenceId> {
    let platform = input.context.platform;
    let Some(home) = &input.context.home else {
        return BTreeSet::new();
    };
    let home = crate::locations::components(home, platform);
    let below_home = !input.paths.is_empty()
        && input.paths.iter().all(|path| {
            let names = crate::locations::components(path, platform);
            names.len() > home.len() && names.starts_with(&home)
        });
    let owned = about(input, |f| *f == Fact::OwnedByCurrentUser(true));
    let disowned = !about(input, |f| *f == Fact::OwnedByCurrentUser(false)).is_empty();
    let quarantine = input
        .context
        .mechanisms
        .contains(&CleanupMechanism::QuarantineRename);
    if below_home && !disowned && quarantine {
        owned
    } else {
        BTreeSet::new()
    }
}

/// Only some processes could be checked, and ADR-0030's conditions do not
/// hold: the item is kept.
fn in_use_partial(input: &PolicyInput) -> Option<Finding> {
    if !partial_allowed(input).is_empty() {
        return None;
    }
    keep_if(partial_answer(input), RiskFactor::IncompleteCoverage)
}

/// Only some processes could be checked, but ADR-0030's conditions hold: the
/// item stays eligible, and the decision records that it relied on a partial
/// answer (the executor must probe again before the rename).
fn in_use_partial_allowed(input: &PolicyInput) -> Option<Finding> {
    let partial = partial_answer(input);
    let owned = partial_allowed(input);
    (!partial.is_empty() && !owned.is_empty()).then(|| Finding {
        verdict: Verdict::Quarantine,
        evidence: partial.union(&owned).copied().collect(),
        risk: BTreeSet::from([RiskFactor::IncompleteCoverage]),
        rule: None,
    })
}

/// Nobody checked whether the item is in use: unknown is never "unused".
fn in_use_unknown(input: &PolicyInput) -> Option<Finding> {
    let answered = about(input, |f| {
        matches!(f, Fact::InUse(_) | Fact::NotInUseByVisibleProcesses { .. })
    });
    answered.is_empty().then(|| Finding {
        verdict: Verdict::Keep,
        evidence: BTreeSet::new(),
        risk: BTreeSet::from([RiskFactor::IncompleteCoverage]),
        rule: None,
    })
}

/// A finding without evidence: the rule fired on the input's shape.
fn finding(verdict: Verdict, risk: &[RiskFactor]) -> Finding {
    Finding {
        verdict,
        evidence: BTreeSet::new(),
        risk: risk.iter().copied().collect(),
        rule: None,
    }
}

/// A file the scan never observed: its size, kind and coverage are unknown.
fn not_observed(input: &PolicyInput) -> Option<Finding> {
    (matches!(input.subject, Subject::File { .. }) && input.observed.is_none())
        .then(|| finding(Verdict::Keep, &[RiskFactor::IncompleteCoverage]))
}

/// Something below the item, or the item itself, was not read: unreadable is
/// never empty (ADR-0008).
fn incomplete_scan(input: &PolicyInput) -> Option<Finding> {
    let observed = input.observed.as_ref()?;
    (!observed.complete || !observed.entry.access.is_observed())
        .then(|| finding(Verdict::Keep, &[RiskFactor::IncompleteCoverage]))
}

/// The platform cannot quarantine here, so nothing can be proposed for it.
fn quarantine_unsupported(input: &PolicyInput) -> Option<Finding> {
    (!input
        .context
        .mechanisms
        .contains(&CleanupMechanism::QuarantineRename))
    .then(|| finding(Verdict::Review, &[]))
}

/// Removing it frees nothing now: hard links elsewhere, clones, snapshots,
/// dataless or empty items are never proposed for space.
fn frees_nothing(input: &PolicyInput) -> Option<Finding> {
    let observed = input.observed.as_ref()?;
    (observed.reclaim.reclaimable_now == ByteCount::ZERO).then(|| finding(Verdict::Review, &[]))
}

/// Links, junctions, reparse points and special files are other objects'
/// doorways, not artifacts to quarantine.
fn object_kind(input: &PolicyInput) -> Option<Finding> {
    let observed = input.observed.as_ref()?;
    (!matches!(observed.entry.kind, EntryKind::File | EntryKind::Directory))
        .then(|| finding(Verdict::Review, &[]))
}

#[cfg(test)]
mod tests {
    use lumen_domain::{
        AccessState, Basis, DenialReason, FileId, FileIdentity, PolicyDecision, Protection,
        Provenance, RiskLevel, SourceName, Subject, Timestamp, VolumeId,
    };

    use super::*;
    use crate::{PolicyError, evaluate};

    fn file(n: u128) -> Subject {
        Subject::File {
            identity: FileIdentity {
                volume: VolumeId::new("v").unwrap_or_else(|_| unreachable!()),
                file: FileId::new(n),
            },
        }
    }

    fn fact(subject: Subject, fact: Fact) -> Evidence {
        Evidence {
            subject,
            fact,
            basis: Basis::Observed,
            provenance: Provenance {
                source: SourceName::new("test").unwrap_or_else(|_| unreachable!()),
                observed_at: Timestamp::UNIX_EPOCH,
            },
        }
    }

    fn decide(facts: &[Fact]) -> Result<PolicyDecision, PolicyError> {
        evaluate(&crate::testing::input(
            facts.iter().map(|f| fact(file(1), f.clone())).collect(),
        ))
    }

    fn fired(decision: &PolicyDecision) -> Vec<&str> {
        decision
            .fired_rules()
            .iter()
            .map(|r| r.rule.as_str())
            .collect()
    }

    const NOT_IN_USE: Fact = Fact::InUse(false);

    #[test]
    fn an_unprotected_idle_item_reaches_classification() -> Result<(), PolicyError> {
        let decision = decide(&[NOT_IN_USE])?;
        assert_eq!(decision.verdict(), Verdict::Review);
        assert_eq!(fired(&decision), ["default.review"]);
        Ok(())
    }

    #[test]
    fn observed_protections_keep() -> Result<(), PolicyError> {
        let sip = Protection {
            system_restricted: true,
            ..Protection::default()
        };
        let immutable = Protection {
            undeletable: true,
            ..Protection::default()
        };
        let system = Protection {
            system_attribute: true,
            ..Protection::default()
        };
        for (protection, rule) in [
            (sip, "protect.os-protected"),
            (immutable, "protect.os-protected"),
            (system, "protect.system-file"),
        ] {
            let decision = decide(&[NOT_IN_USE, Fact::Protected(protection)])?;
            assert_eq!(decision.verdict(), Verdict::Keep);
            assert_eq!(fired(&decision), [rule]);
            assert_eq!(decision.risk().level, RiskLevel::Critical);
        }
        let hidden = Protection {
            hidden: true,
            ..Protection::default()
        };
        assert_eq!(
            decide(&[NOT_IN_USE, Fact::Protected(hidden)])?.verdict(),
            Verdict::Review,
            "hidden alone is not a protection"
        );
        let cloud = decide(&[NOT_IN_USE, Fact::CloudBacked(true)])?;
        assert_eq!(fired(&cloud), ["protect.cloud-backed"]);
        assert_eq!(cloud.verdict(), Verdict::Keep);
        Ok(())
    }

    #[test]
    fn only_a_complete_no_passes_the_in_use_gate() -> Result<(), PolicyError> {
        let used = decide(&[Fact::InUse(true)])?;
        assert_eq!(fired(&used), ["protect.in-use"]);
        let partial = decide(&[Fact::NotInUseByVisibleProcesses { unseen: Some(2) }])?;
        assert_eq!(fired(&partial), ["protect.in-use-partial"]);
        assert_eq!(partial.verdict(), Verdict::Keep);
        let unknown = decide(&[])?;
        assert_eq!(fired(&unknown), ["protect.in-use-unknown"]);
        assert_eq!(unknown.verdict(), Verdict::Keep);
        let complete = decide(&[
            Fact::NotInUseByVisibleProcesses { unseen: None },
            NOT_IN_USE,
        ])?;
        assert_eq!(complete.verdict(), Verdict::Review, "a complete no wins");
        let conflicting = decide(&[NOT_IN_USE, Fact::InUse(true)])?;
        assert_eq!(fired(&conflicting), ["protect.in-use"], "any yes keeps");
        Ok(())
    }

    fn decide_with(edit: impl FnOnce(&mut PolicyInput)) -> Result<PolicyDecision, PolicyError> {
        let mut input = crate::testing::input(vec![fact(file(1), NOT_IN_USE)]);
        edit(&mut input);
        evaluate(&input)
    }

    #[test]
    fn unobserved_or_partly_read_items_are_kept() -> Result<(), PolicyError> {
        let unobserved = decide_with(|i| i.observed = None)?;
        assert_eq!(unobserved.verdict(), Verdict::Keep);
        assert_eq!(fired(&unobserved), ["protect.not-observed"]);
        let partial = decide_with(|i| {
            if let Some(o) = i.observed.as_mut() {
                o.complete = false;
            }
        })?;
        assert_eq!(fired(&partial), ["protect.incomplete-scan"]);
        let denied = decide_with(|i| {
            if let Some(o) = i.observed.as_mut() {
                o.entry.access = AccessState::Denied(DenialReason::Posix);
            }
        })?;
        assert_eq!(fired(&denied), ["protect.incomplete-scan"]);
        assert_eq!(denied.risk().level, RiskLevel::Medium);
        Ok(())
    }

    #[test]
    fn ineligible_items_are_at_most_review() -> Result<(), PolicyError> {
        let unsupported = decide_with(|i| i.context.mechanisms.clear())?;
        assert_eq!(unsupported.verdict(), Verdict::Review);
        assert!(fired(&unsupported).contains(&"eligible.quarantine-unsupported"));
        let nothing = decide_with(|i| {
            i.observed = Some(crate::testing::observed(EntryKind::File, 0));
        })?;
        assert!(fired(&nothing).contains(&"eligible.frees-nothing"));
        let link = decide_with(|i| {
            i.observed = Some(crate::testing::observed(EntryKind::Symlink, 10));
        })?;
        assert!(fired(&link).contains(&"eligible.object-kind"));
        let directory = decide_with(|i| {
            i.observed = Some(crate::testing::observed(EntryKind::Directory, 10));
        })?;
        assert_eq!(fired(&directory), ["default.review"], "eligible");
        Ok(())
    }

    #[test]
    fn non_file_subjects_need_no_scan_observation() -> Result<(), PolicyError> {
        let app = Subject::Application {
            id: lumen_domain::AppId {
                scheme: lumen_domain::AppIdScheme::BundleId,
                value: lumen_domain::UntrustedText::new("com.example.app"),
            },
        };
        let mut input = crate::testing::input(vec![fact(app.clone(), NOT_IN_USE)]);
        input.subject = app;
        input.observed = None;
        let decision = evaluate(&input)?;
        assert!(!fired(&decision).contains(&"protect.not-observed"));
        Ok(())
    }

    /// A partially checked item at `path`, with `facts`.
    fn partial_at(path: &str, facts: &[Fact]) -> Result<PolicyDecision, PolicyError> {
        let mut evidence = vec![fact(
            file(1),
            Fact::NotInUseByVisibleProcesses { unseen: None },
        )];
        evidence.extend(facts.iter().map(|f| fact(file(1), f.clone())));
        let mut input = crate::testing::input(evidence);
        input.paths = vec![
            lumen_domain::RawPath::from_unix_bytes(path.as_bytes().to_vec())
                .unwrap_or_else(|_| unreachable!()),
        ];
        evaluate(&input)
    }

    #[test]
    fn a_partial_answer_passes_only_under_adr_0030() -> Result<(), PolicyError> {
        let owned = Fact::OwnedByCurrentUser(true);
        let allowed = partial_at("/home/t/.cache/app", std::slice::from_ref(&owned))?;
        assert_eq!(
            fired(&allowed),
            ["default.review", "eligible.in-use-partial"]
        );
        assert_eq!(
            allowed.verdict(),
            Verdict::Review,
            "eligible; nothing classified it"
        );
        let mut complete = crate::testing::input(vec![
            fact(file(1), NOT_IN_USE),
            fact(file(1), owned.clone()),
        ]);
        complete.paths = vec![
            lumen_domain::RawPath::from_unix_bytes(b"/home/t/.cache/app".to_vec())
                .unwrap_or_else(|_| unreachable!()),
        ];
        assert_eq!(
            fired(&evaluate(&complete)?),
            ["default.review"],
            "a complete answer relies on nothing partial"
        );
        let relied = allowed
            .fired_rules()
            .iter()
            .find(|r| r.rule.as_str() == "eligible.in-use-partial")
            .map(|r| r.evidence.len());
        assert_eq!(
            relied,
            Some(2),
            "cites the partial answer and the ownership"
        );

        for (path, facts) in [
            ("/home/t/.cache/app", vec![]),
            ("/home/t/.cache/app", vec![Fact::OwnedByCurrentUser(false)]),
            (
                "/home/t/.cache/app",
                vec![owned.clone(), Fact::OwnedByCurrentUser(false)],
            ),
            ("/srv/shared/app", vec![owned.clone()]),
            ("/home/t", vec![owned.clone()]),
            ("/home/tom/x", vec![owned.clone()]),
        ] {
            let decision = partial_at(path, &facts)?;
            assert_eq!(decision.verdict(), Verdict::Keep, "{path} {facts:?}");
            assert_eq!(fired(&decision), ["protect.in-use-partial"]);
        }
        Ok(())
    }

    #[test]
    fn evidence_about_other_subjects_is_ignored() -> Result<(), PolicyError> {
        let decision = evaluate(&crate::testing::input(vec![
            fact(file(1), NOT_IN_USE),
            fact(file(2), Fact::InUse(true)),
            fact(file(2), Fact::CloudBacked(true)),
        ]))?;
        assert_eq!(decision.verdict(), Verdict::Review);
        Ok(())
    }

    #[test]
    fn protections_cite_their_evidence() -> Result<(), PolicyError> {
        let used = fact(file(1), Fact::InUse(true));
        let decision = evaluate(&crate::testing::input(vec![used.clone()]))?;
        let rule = decision
            .fired_rules()
            .first()
            .ok_or_else(|| PolicyError::RuleId("none".into()))?;
        assert_eq!(rule.evidence, BTreeSet::from([used.id()]));
        Ok(())
    }
}
