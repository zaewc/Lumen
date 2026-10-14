//! Protected-fixture gate (roadmap 8.7d, docs/ai/evaluation.md "Policy safety").
//!
//! Every protected stratum in docs/ai/datasets.md is decided with an
//! adversarial judge that always answers "likely removable, high confidence".
//! Protected items must stay `Keep` and must never be sent to the judge;
//! unprotected items the policy leaves at `Review` must not be promoted.

use std::collections::{BTreeMap, BTreeSet};
use std::num::NonZeroU32;
use std::task::{Context, Poll, Waker};

use lumen_application::consult::{Consulted, decide};
use lumen_application::ports::{Clock, JudgeError, JudgeFuture, JudgeModel};
use lumen_domain::{
    AccessState, Assessment, Basis, ByteCount, CandidateSummary, Category, CleanupMechanism,
    CoverageSummary, EntryKind, EntryTimes, Evidence, EvidenceHash, Fact, FileId, FileIdentity,
    FilesystemEntry, ItemRef, JevRequest, JudgeDescriptor, Judgment, JudgmentConfidence, ModelId,
    Platform, Protection, Provenance, RawPath, ReasonCode, ReclaimEstimate, SizeFacts, SizeFlags,
    SourceName, Subject, Timestamp, UntrustedText, Verdict, VolumeId,
};
use lumen_policy::{Context as PolicyContext, Observed, PolicyInput};

/// Answers every request "likely removable, high", and counts the requests.
#[derive(Default)]
struct Adversary {
    asked: std::sync::Mutex<usize>,
}

impl JudgeModel for Adversary {
    fn descriptor(&self) -> Option<JudgeDescriptor> {
        Some(JudgeDescriptor {
            provider: SourceName::new("adversary").unwrap_or_else(|_| unreachable!()),
            model: ModelId::new("adversary-1").unwrap_or_else(|_| unreachable!()),
            model_version: None,
            prompt_version: "adversary/1".parse().unwrap_or_else(|_| unreachable!()),
        })
    }

    fn judge<'a>(&'a self, request: &'a JevRequest) -> JudgeFuture<'a> {
        *self
            .asked
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) += 1;
        let answer: Result<Judgment, JudgeError> = Ok(Judgment {
            item_ref: request.item_ref.clone(),
            assessment: Assessment::LikelyRemovable,
            confidence: JudgmentConfidence::High,
            reason_codes: BTreeSet::from([ReasonCode::RegeneratableCache]),
            evidence_refs: request.evidence_ids(),
            injection_suspected: false,
            rationale: UntrustedText::new("adversarial"),
        });
        Box::pin(std::future::ready(answer))
    }
}

struct Epoch;

impl Clock for Epoch {
    fn now(&self) -> Timestamp {
        Timestamp::UNIX_EPOCH
    }
}

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
            source: SourceName::new("fixture").unwrap_or_else(|_| unreachable!()),
            observed_at: Timestamp::UNIX_EPOCH,
        },
    }
}

fn unix(path: &str) -> RawPath {
    RawPath::from_unix_bytes(path.as_bytes().to_vec()).unwrap_or_else(|_| unreachable!())
}

/// An observed, fully read, free item at `path` on macOS with `facts`.
fn input(path: &str, facts: Vec<Fact>) -> PolicyInput {
    PolicyInput {
        subject: subject(),
        evidence_hash: EvidenceHash::from_bytes([1; 32]),
        evidence: facts.into_iter().map(evidence).collect(),
        paths: vec![unix(path)],
        context: PolicyContext {
            platform: Platform::Macos,
            home: Some(unix("/Users/ana")),
            windows_dir: None,
            local_app_data: None,
            mechanisms: BTreeSet::from([CleanupMechanism::QuarantineRename]),
            complete_sources: BTreeSet::new(),
        },
        observed: Some(Observed {
            entry: FilesystemEntry {
                path: unix(path),
                kind: EntryKind::Directory,
                size: SizeFacts {
                    identity: FileIdentity {
                        volume: VolumeId::new("v").unwrap_or_else(|_| unreachable!()),
                        file: FileId::new(1),
                    },
                    logical: ByteCount::new(4096),
                    allocated: ByteCount::new(4096),
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
                reclaimable_now: ByteCount::new(4096),
                ..ReclaimEstimate::default()
            },
        }),
        knowledge: BTreeMap::new(),
        jev: None,
    }
}

fn request() -> JevRequest {
    JevRequest {
        schema_version: JevRequest::current_schema().unwrap_or_else(|_| unreachable!()),
        item_ref: ItemRef::new("fixture").unwrap_or_else(|_| unreachable!()),
        candidate: CandidateSummary {
            category: Category::Other,
            path_display: UntrustedText::new("~/x"),
            size_bytes: ByteCount::new(4096),
            reclaimable_bytes: ByteCount::new(4096),
        },
        evidence: Vec::new(),
        coverage: CoverageSummary {
            complete: true,
            notes: BTreeSet::new(),
        },
        untrusted_strings: BTreeMap::new(),
    }
}

fn decided(judge: &Adversary, input: PolicyInput) -> Consulted {
    let request = request();
    let mut future = Box::pin(decide(judge, &Epoch, input, &request));
    match future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(result) => result.unwrap_or_else(|e| unreachable!("{e}")),
        Poll::Pending => unreachable!("the adversary answers immediately"),
    }
}

#[test]
fn protected_strata_stay_kept_and_never_reach_the_judge() {
    let free = || Fact::InUse(false);
    let fixtures: Vec<(&str, PolicyInput)> = vec![
        (
            "system file",
            input("/System/Library/Caches/com.apple.x", vec![free()]),
        ),
        ("credentials", input("/Users/ana/.ssh", vec![free()])),
        (
            "keychains",
            input("/Users/ana/Library/Keychains", vec![free()]),
        ),
        (
            "browser profile",
            input(
                "/Users/ana/Library/Application Support/Google/Chrome",
                vec![free()],
            ),
        ),
        (
            "cloud placeholder",
            input(
                "/Users/ana/Library/Mobile Documents/x",
                vec![free(), Fact::CloudBacked(true)],
            ),
        ),
        (
            "active cache",
            input(
                "/Users/ana/Library/Caches/com.example.App",
                vec![Fact::InUse(true)],
            ),
        ),
        (
            "os protected",
            input(
                "/Users/ana/x",
                vec![
                    free(),
                    Fact::Protected(Protection {
                        system_restricted: true,
                        ..Protection::default()
                    }),
                ],
            ),
        ),
        (
            "unchecked use",
            input("/Users/ana/Library/Caches/com.example.App", vec![]),
        ),
    ];
    for (stratum, fixture) in fixtures {
        let judge = Adversary::default();
        let consulted = decided(&judge, fixture);
        assert_eq!(consulted.decision.verdict(), Verdict::Keep, "{stratum}");
        assert_eq!(consulted.trace, None, "{stratum}: no trace");
        assert_eq!(
            *judge
                .asked
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
            0,
            "{stratum}: never sent to the judge"
        );
    }
}

#[test]
fn an_adversarial_judge_never_promotes_review() {
    let judge = Adversary::default();
    let consulted = decided(
        &judge,
        input("/Users/ana/Documents/notes", vec![Fact::InUse(false)]),
    );
    assert_eq!(consulted.decision.verdict(), Verdict::Review);
    assert_eq!(
        *judge
            .asked
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner),
        1,
        "asked, because the item is not protected"
    );
}
