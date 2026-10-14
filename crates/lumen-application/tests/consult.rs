//! Consulting Jev: when it is asked, what it can change, and what is traced.

use std::collections::{BTreeMap, BTreeSet};
use std::num::NonZeroU32;
use std::sync::Mutex;
use std::task::{Context, Poll, Waker};

use lumen_application::consult::{Consulted, decide};
use lumen_application::ports::{Clock, JudgeError};
use lumen_domain::{
    AccessState, Assessment, Basis, ByteCount, CandidateSummary, Category, CleanupMechanism,
    CoverageSummary, EntryKind, EntryTimes, Evidence, EvidenceHash, Fact, FileId, FileIdentity,
    FilesystemEntry, ItemRef, JevEffect, JevRequest, JudgeDescriptor, Judgment, JudgmentConfidence,
    JudgmentRejection, ModelId, Platform, PromptVersion, Protection, Provenance, RawPath,
    ReasonCode, ReclaimEstimate, RequestEvidence, SizeFacts, SizeFlags, SourceName, Subject,
    Timestamp, TraceValidation, UntrustedText, Verdict, VolumeId,
};
use lumen_policy::{Context as PolicyContext, KnowledgeClass, Observed, PolicyInput};
use lumen_testkit::MockJudge;

/// A clock that advances 25 ms per reading.
struct Ticking(Mutex<i64>);

impl Clock for Ticking {
    fn now(&self) -> Timestamp {
        let mut ms = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *ms += 25;
        Timestamp::from_unix(
            *ms / 1000,
            u32::try_from(*ms % 1000).unwrap_or(0) * 1_000_000,
        )
        .unwrap_or(Timestamp::UNIX_EPOCH)
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
            source: SourceName::new("test").unwrap_or_else(|_| unreachable!()),
            observed_at: Timestamp::UNIX_EPOCH,
        },
    }
}

fn entry_name() -> SourceName {
    SourceName::new("x-cache").unwrap_or_else(|_| unreachable!())
}

/// A free, fully read, regenerable cache: the policy proposes quarantine.
fn input(in_use: bool) -> PolicyInput {
    let home = RawPath::from_unix_bytes(b"/home/t".to_vec()).unwrap_or_else(|_| unreachable!());
    PolicyInput {
        subject: subject(),
        evidence_hash: EvidenceHash::from_bytes([5; 32]),
        evidence: vec![
            evidence(Fact::InUse(in_use)),
            evidence(Fact::KnowledgeBaseMatch {
                entry: entry_name(),
            }),
        ],
        paths: Vec::new(),
        context: PolicyContext {
            platform: Platform::Linux,
            home: Some(home.clone()),
            windows_dir: None,
            local_app_data: None,
            mechanisms: BTreeSet::from([CleanupMechanism::QuarantineRename]),
            complete_sources: BTreeSet::new(),
        },
        observed: Some(Observed {
            entry: FilesystemEntry {
                path: home,
                kind: EntryKind::Directory,
                size: SizeFacts {
                    identity: FileIdentity {
                        volume: VolumeId::new("v").unwrap_or_else(|_| unreachable!()),
                        file: FileId::new(1),
                    },
                    logical: ByteCount::new(10),
                    allocated: ByteCount::new(10),
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
                reclaimable_now: ByteCount::new(10),
                ..ReclaimEstimate::default()
            },
        }),
        knowledge: BTreeMap::from([(entry_name(), KnowledgeClass::Regenerable)]),
        jev: None,
    }
}

fn request() -> JevRequest {
    JevRequest {
        schema_version: JevRequest::current_schema().unwrap_or_else(|_| unreachable!()),
        item_ref: ItemRef::new("c_1").unwrap_or_else(|_| unreachable!()),
        candidate: CandidateSummary {
            category: Category::DeveloperCache,
            path_display: UntrustedText::new("~/x"),
            size_bytes: ByteCount::new(10),
            reclaimable_bytes: ByteCount::new(10),
        },
        evidence: vec![RequestEvidence {
            id: evidence(Fact::InUse(false)).id(),
            kind: lumen_domain::EvidenceKind::ActiveProcessReference,
            value: UntrustedText::new("false"),
        }],
        coverage: CoverageSummary {
            complete: true,
            notes: BTreeSet::new(),
        },
        untrusted_strings: BTreeMap::new(),
    }
}

fn descriptor() -> JudgeDescriptor {
    JudgeDescriptor {
        provider: SourceName::new("mock").unwrap_or_else(|_| unreachable!()),
        model: ModelId::new("mock-1").unwrap_or_else(|_| unreachable!()),
        model_version: None,
        prompt_version: PromptVersion::new("jev-prompt", 1).unwrap_or_else(|_| unreachable!()),
    }
}

fn judgment(assessment: Assessment) -> Judgment {
    Judgment {
        item_ref: ItemRef::new("c_1").unwrap_or_else(|_| unreachable!()),
        assessment,
        confidence: JudgmentConfidence::High,
        reason_codes: BTreeSet::from([ReasonCode::OwnerAppPresent]),
        evidence_refs: BTreeSet::from([evidence(Fact::InUse(false)).id()]),
        injection_suspected: false,
        rationale: UntrustedText::new("scripted"),
    }
}

fn run(judge: &MockJudge, input: PolicyInput) -> Consulted {
    let clock = Ticking(Mutex::new(0));
    let request = request();
    let mut future = Box::pin(decide(judge, &clock, input, &request));
    match future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(result) => result.unwrap_or_else(|e| unreachable!("{e}")),
        Poll::Pending => unreachable!("the mock judge answers immediately"),
    }
}

fn scripted(answer: Result<Judgment, JudgeError>) -> MockJudge {
    MockJudge::new(Some(descriptor())).answer(
        ItemRef::new("c_1").unwrap_or_else(|_| unreachable!()),
        answer,
    )
}

#[test]
fn a_disabled_judge_is_never_asked() {
    let consulted = run(&MockJudge::new(None), input(false));
    assert_eq!(consulted.decision.verdict(), Verdict::Quarantine);
    assert_eq!(consulted.trace, None);
}

#[test]
fn kept_items_never_reach_jev() {
    let consulted = run(
        &scripted(Ok(judgment(Assessment::LikelyRemovable))),
        input(true),
    );
    assert_eq!(consulted.decision.verdict(), Verdict::Keep);
    assert_eq!(consulted.trace, None);
}

#[test]
fn a_cautious_judgment_demotes_and_is_traced() {
    let consulted = run(
        &scripted(Ok(judgment(Assessment::LikelyNeeded))),
        input(false),
    );
    assert_eq!(consulted.decision.verdict(), Verdict::Review);
    let Some(trace) = consulted.trace else {
        unreachable!("consulted");
    };
    assert_eq!(trace.validation, TraceValidation::Accepted);
    assert_eq!(trace.policy_effect, JevEffect::DemotedToReview);
    assert_eq!(trace.request_hash, request().hash());
    assert_eq!(trace.evidence_hash, EvidenceHash::from_bytes([5; 32]));
    assert_eq!(trace.latency_ms, 25);
    assert!(trace.judgment.is_some());
}

#[test]
fn errors_leave_the_decision_unchanged_and_are_traced() {
    for (answer, validation) in [
        (
            Err(JudgeError::Rejected(JudgmentRejection::UnknownEvidence)),
            TraceValidation::Rejected(JudgmentRejection::UnknownEvidence),
        ),
        (Err(JudgeError::Malformed), TraceValidation::Unavailable),
        (
            Err(JudgeError::Unavailable("offline")),
            TraceValidation::Unavailable,
        ),
    ] {
        let consulted = run(&scripted(answer), input(false));
        assert_eq!(consulted.decision.verdict(), Verdict::Quarantine);
        let trace = consulted.trace.unwrap_or_else(|| unreachable!("consulted"));
        assert_eq!(trace.validation, validation);
        assert_eq!(trace.policy_effect, JevEffect::None);
        assert_eq!(trace.judgment, None);
    }
}

#[test]
fn a_removable_judgment_changes_nothing() {
    let consulted = run(
        &scripted(Ok(judgment(Assessment::LikelyRemovable))),
        input(false),
    );
    assert_eq!(consulted.decision.verdict(), Verdict::Quarantine);
    assert_eq!(
        consulted.trace.map(|t| t.policy_effect),
        Some(JevEffect::None)
    );
}
