//! Describing items left for review (ADR-0041).
//!
//! While AI review is on, the judge is asked what kind of item each of up to
//! [`MAX_DESCRIBED`] review items most likely is. The answer is a note for
//! people, set on the assessment beside its decision. The policy, plans and
//! the executor never read it, so every decision is the same with or without
//! it.
//!
//! Only items the policy itself left at `REVIEW` (the judge was not consulted
//! about them) and that no knowledge-base entry identifies are described, the
//! largest first. Requests go one at a time; the first answer that says the
//! judge is unavailable (unreachable, timed out, rate limit, quota) ends
//! describing for this assessment. Every request is traced (`jev.trace/1`
//! with output schema `jev.item-note/1`), accepted or not.

use std::cmp::Reverse;

use lumen_domain::{
    ByteCount, Fact, ItemNote, JevEffect, JevRequest, JevTrace, SchemaVersion, TraceValidation,
    Verdict,
};
use lumen_policy::PolicyInput;

use crate::assess::Assessed;
use crate::consult::millis;
use crate::ports::{Clock, Describer, JudgeError};

/// The most review items described in one assessment.
pub const MAX_DESCRIBED: usize = 20;

/// Whether `assessed` may be described: the policy left it at `REVIEW`
/// without consulting the judge, and no knowledge-base entry says what it is.
pub fn describable(assessed: &Assessed) -> bool {
    let input = &assessed.input;
    let known = input
        .evidence
        .iter()
        .any(|e| e.subject == input.subject && matches!(e.fact, Fact::KnowledgeBaseMatch { .. }));
    assessed.decision.verdict() == Verdict::Review
        && assessed.decision.jev_effect() == JevEffect::None
        && assessed.trace.is_none()
        && !known
}

/// The larger of the item's allocated size and the space removing it frees.
fn size(assessed: &Assessed) -> ByteCount {
    assessed
        .input
        .observed
        .as_ref()
        .map_or(ByteCount::ZERO, |o| {
            o.reclaim.allocated.max(o.reclaim.reclaimable_now)
        })
}

/// The positions of the items to describe: describable ones, the largest
/// first (then by path), at most [`MAX_DESCRIBED`].
pub fn chosen(assessed: &[Assessed]) -> Vec<usize> {
    let mut chosen: Vec<usize> = (0..assessed.len())
        .filter(|&i| assessed.get(i).is_some_and(describable))
        .collect();
    chosen.sort_by_key(|&i| {
        assessed
            .get(i)
            .map(|a| (Reverse(size(a)), a.candidate.path.clone()))
    });
    chosen.truncate(MAX_DESCRIBED);
    chosen
}

/// Asks `describer` about the chosen items of `assessed`, one at a time, and
/// sets each one's note (when it may be shown) and trace. Decisions, inputs
/// and reasons are left exactly as they were. With AI review off (no
/// descriptor), nothing is asked.
pub async fn describe(
    describer: &dyn Describer,
    clock: &dyn Clock,
    assessed: &mut [Assessed],
    request: &dyn Fn(&PolicyInput) -> Option<JevRequest>,
) {
    describe_with_progress(describer, clock, assessed, request, &mut |_, _| {}).await;
}

/// Like [`describe`], reporting attempted items out of the selected total.
/// An unavailable judge ends the stage early; unattempted items are never
/// counted as completed.
pub async fn describe_with_progress(
    describer: &dyn Describer,
    clock: &dyn Clock,
    assessed: &mut [Assessed],
    request: &dyn Fn(&PolicyInput) -> Option<JevRequest>,
    progress: &mut dyn FnMut(usize, usize),
) {
    let Some(descriptor) = describer.descriptor() else {
        return;
    };
    let output_schema = SchemaVersion::new("jev.item-note", 1, 0)
        .unwrap_or_else(|e| unreachable!("constant schema version is valid: {e}"));
    let chosen = chosen(assessed);
    let total = chosen.len();
    progress(0, total);
    for (completed, index) in chosen.into_iter().enumerate() {
        let Some(item) = assessed.get_mut(index) else {
            continue;
        };
        let Some(jev_request) = request(&item.input) else {
            progress(completed + 1, total);
            continue;
        };
        let requested_at = clock.now();
        let answer = describer.describe(&jev_request).await;
        let latency_ms = millis(requested_at, clock.now());
        let stop = matches!(
            answer,
            Err(JudgeError::Unavailable(_) | JudgeError::Disabled)
        );
        let (validation, note) = match answer {
            Ok(note) => (TraceValidation::Accepted, Some(note)),
            Err(JudgeError::Rejected(reason)) => (TraceValidation::Rejected(reason), None),
            Err(_) => (TraceValidation::Unavailable, None),
        };
        item.note = note.filter(ItemNote::shown);
        item.note_trace = Some(JevTrace {
            requested_at,
            judge: descriptor.clone(),
            policy_version: item.decision.policy_version(),
            input_schema: jev_request.schema_version.clone(),
            output_schema: output_schema.clone(),
            evidence_hash: item.input.evidence_hash,
            request_hash: jev_request.hash(),
            response_hash: None,
            validation,
            judgment: None,
            calibrated_precision_bp: None,
            policy_effect: JevEffect::None,
            latency_ms,
            note,
        });
        progress(completed + 1, total);
        if stop {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};
    use std::num::NonZeroU32;
    use std::sync::Mutex;
    use std::task::{Context as TaskContext, Poll, Waker};

    use lumen_domain::{
        AccessState, Basis, CandidateSummary, Category, CleanupMechanism, CoverageSummary,
        EntryKind, EntryTimes, Evidence, EvidenceHash, FileId, FileIdentity, FilesystemEntry,
        ItemKind, ItemRef, JudgeDescriptor, JudgmentConfidence, ModelId, Platform, Protection,
        Provenance, RawPath, ReclaimEstimate, SizeFacts, SizeFlags, SourceName, Subject, Timestamp,
        UntrustedText, VolumeId,
    };
    use lumen_policy::{Context, KnowledgeClass, Observed, evaluate, explain};

    use super::*;
    use crate::ports::{NoopJudge, NoteFuture};

    /// Answers every request with `answers` in turn (the last one repeats),
    /// and records the item references asked about.
    struct Fake {
        answers: Vec<Result<ItemNote, JudgeError>>,
        asked: Mutex<Vec<String>>,
    }

    impl Fake {
        fn new(answers: Vec<Result<ItemNote, JudgeError>>) -> Self {
            Self {
                answers,
                asked: Mutex::new(Vec::new()),
            }
        }

        fn asked(&self) -> Vec<String> {
            self.asked
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .clone()
        }
    }

    impl Describer for Fake {
        fn descriptor(&self) -> Option<JudgeDescriptor> {
            Some(JudgeDescriptor {
                provider: SourceName::new("fake").unwrap_or_else(|_| unreachable!()),
                model: ModelId::new("fake-1").unwrap_or_else(|_| unreachable!()),
                model_version: None,
                prompt_version: "jev-describe/1".parse().unwrap_or_else(|_| unreachable!()),
            })
        }

        fn describe<'a>(&'a self, request: &'a JevRequest) -> NoteFuture<'a> {
            let mut asked = self
                .asked
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let answer = self
                .answers
                .get(asked.len())
                .or(self.answers.last())
                .cloned()
                .unwrap_or(Err(JudgeError::Malformed));
            asked.push(request.item_ref.as_str().to_owned());
            Box::pin(std::future::ready(answer))
        }
    }

    struct Epoch;

    impl Clock for Epoch {
        fn now(&self) -> Timestamp {
            Timestamp::UNIX_EPOCH
        }
    }

    fn ready<T>(future: impl Future<Output = T>) -> T {
        let mut future = std::pin::pin!(future);
        match future
            .as_mut()
            .poll(&mut TaskContext::from_waker(Waker::noop()))
        {
            Poll::Ready(value) => value,
            Poll::Pending => unreachable!("the fakes answer at once"),
        }
    }

    fn identity(n: u128) -> FileIdentity {
        FileIdentity {
            volume: VolumeId::new("v").unwrap_or_else(|_| unreachable!()),
            file: FileId::new(n),
        }
    }

    fn path(n: u128) -> RawPath {
        RawPath::from_unix_bytes(format!("/home/t/item-{n:03}").into_bytes())
            .unwrap_or_else(|_| unreachable!())
    }

    /// A shape of item and the verdict the policy gives it.
    #[derive(Debug, Clone, Copy)]
    enum Shape {
        /// Free and unrecognised: `REVIEW` when large, `KEEP` when small.
        Unknown,
        /// In use: `KEEP`.
        InUse,
        /// A regenerable knowledge-base match: `QUARANTINE`.
        Regenerable,
        /// A not-regenerable knowledge-base match: `REVIEW`, but known.
        NotRegenerable,
    }

    /// Item `n` of `bytes`, decided by the policy.
    fn item(n: u128, bytes: u64, shape: Shape) -> Assessed {
        let subject = Subject::File {
            identity: identity(n),
        };
        let fact = |fact: Fact| Evidence {
            subject: subject.clone(),
            fact,
            basis: Basis::Observed,
            provenance: Provenance {
                source: SourceName::new("test").unwrap_or_else(|_| unreachable!()),
                observed_at: Timestamp::UNIX_EPOCH,
            },
        };
        let entry_name = SourceName::new("tool-data").unwrap_or_else(|_| unreachable!());
        let mut evidence = vec![fact(Fact::InUse(matches!(shape, Shape::InUse)))];
        let mut knowledge = BTreeMap::new();
        if let Shape::Regenerable | Shape::NotRegenerable = shape {
            evidence.push(fact(Fact::KnowledgeBaseMatch {
                entry: entry_name.clone(),
            }));
            let class = if matches!(shape, Shape::Regenerable) {
                KnowledgeClass::Regenerable
            } else {
                KnowledgeClass::NotRegenerable
            };
            knowledge.insert(entry_name, class);
        }
        let candidate = FilesystemEntry {
            path: path(n),
            kind: EntryKind::Directory,
            size: SizeFacts {
                identity: identity(n),
                logical: ByteCount::new(bytes),
                allocated: ByteCount::new(bytes),
                private: None,
                clone_id: None,
                link_count: NonZeroU32::MIN,
                flags: SizeFlags::default(),
            },
            times: EntryTimes::default(),
            protection: Protection::default(),
            access: AccessState::Readable,
        };
        let input = PolicyInput {
            subject,
            evidence_hash: EvidenceHash::from_bytes([1; 32]),
            evidence,
            paths: vec![path(n)],
            context: Context {
                platform: Platform::Linux,
                home: Some(
                    RawPath::from_unix_bytes(b"/home/t".to_vec())
                        .unwrap_or_else(|_| unreachable!()),
                ),
                windows_dir: None,
                local_app_data: None,
                mechanisms: BTreeSet::from([CleanupMechanism::QuarantineRename]),
                complete_sources: BTreeSet::new(),
                single_user: false,
            },
            observed: Some(Observed {
                entry: candidate.clone(),
                complete: true,
                reclaim: ReclaimEstimate {
                    allocated: ByteCount::new(bytes),
                    reclaimable_now: ByteCount::new(bytes),
                    files: 1,
                    exact: true,
                    ..ReclaimEstimate::default()
                },
            }),
            knowledge,
            jev: None,
        };
        let decision = evaluate(&input).unwrap_or_else(|e| unreachable!("{e}"));
        Assessed {
            candidate,
            reasons: explain(&decision),
            decision,
            trace: None,
            input,
            note: None,
            note_trace: None,
        }
    }

    const LARGE: u64 = 1_000_000_000;

    fn request(input: &PolicyInput) -> Option<JevRequest> {
        let Subject::File { identity } = &input.subject else {
            return None;
        };
        Some(JevRequest {
            schema_version: JevRequest::current_schema().ok()?,
            item_ref: ItemRef::new(format!("item-{}", identity.file.get())).ok()?,
            candidate: CandidateSummary {
                category: Category::Other,
                path_display: UntrustedText::new("~/x"),
                size_bytes: ByteCount::ZERO,
                reclaimable_bytes: ByteCount::ZERO,
            },
            evidence: Vec::new(),
            coverage: CoverageSummary {
                complete: true,
                notes: BTreeSet::new(),
            },
            untrusted_strings: BTreeMap::new(),
        })
    }

    fn note(kind: ItemKind) -> ItemNote {
        ItemNote {
            kind,
            confidence: JudgmentConfidence::Low,
            injection_suspected: false,
        }
    }

    const KINDS: [ItemKind; 8] = [
        ItemKind::AppSettings,
        ItemKind::UserDocuments,
        ItemKind::Cache,
        ItemKind::Logs,
        ItemKind::DownloadsOrInstallers,
        ItemKind::DeveloperToolData,
        ItemKind::BackupOrArchive,
        ItemKind::Unknown,
    ];

    fn mixed() -> Vec<Assessed> {
        vec![
            item(1, LARGE, Shape::Unknown),
            item(2, 10, Shape::Unknown),
            item(3, LARGE, Shape::InUse),
            item(4, LARGE, Shape::Regenerable),
            item(5, LARGE, Shape::NotRegenerable),
        ]
    }

    #[test]
    fn the_fixtures_cover_every_verdict() {
        let verdicts: Vec<Verdict> = mixed().iter().map(|a| a.decision.verdict()).collect();
        assert_eq!(
            verdicts,
            [
                Verdict::Review,
                Verdict::Keep,
                Verdict::Keep,
                Verdict::Quarantine,
                Verdict::Review
            ]
        );
    }

    #[test]
    fn nothing_is_asked_while_ai_review_is_off() {
        let mut assessed = mixed();
        let before = assessed.clone();
        ready(describe(&NoopJudge, &Epoch, &mut assessed, &request));
        assert_eq!(assessed, before);
    }

    #[test]
    fn only_unrecognised_review_items_are_described() {
        let describer = Fake::new(vec![Ok(note(ItemKind::Cache))]);
        let mut assessed = mixed();
        ready(describe(&describer, &Epoch, &mut assessed, &request));
        assert_eq!(describer.asked(), ["item-1"]);
        let notes: Vec<Option<ItemKind>> =
            assessed.iter().map(|a| a.note.map(|n| n.kind)).collect();
        assert_eq!(notes, [Some(ItemKind::Cache), None, None, None, None]);
        let traced = assessed.iter().filter(|a| a.note_trace.is_some()).count();
        assert_eq!(traced, 1);
        let trace = assessed.first().and_then(|a| a.note_trace.as_ref());
        assert_eq!(
            trace.map(|t| (t.output_schema.to_string(), t.validation, t.policy_effect)),
            Some((
                "jev.item-note/1.0".to_owned(),
                TraceValidation::Accepted,
                JevEffect::None
            ))
        );
        assert_eq!(trace.and_then(|t| t.judgment.clone()), None);
    }

    #[test]
    fn a_note_never_changes_a_decision() {
        for kind in KINDS {
            for confidence in [
                JudgmentConfidence::Low,
                JudgmentConfidence::Medium,
                JudgmentConfidence::High,
            ] {
                for injection_suspected in [false, true] {
                    let answer = ItemNote {
                        kind,
                        confidence,
                        injection_suspected,
                    };
                    let describer = Fake::new(vec![Ok(answer)]);
                    let mut assessed = mixed();
                    let before = mixed();
                    ready(describe(&describer, &Epoch, &mut assessed, &request));
                    for (after, before) in assessed.iter().zip(&before) {
                        assert_eq!(after.decision, before.decision, "{answer:?}");
                        assert_eq!(after.input, before.input, "{answer:?}");
                        assert_eq!(after.reasons, before.reasons, "{answer:?}");
                        assert_eq!(after.trace, before.trace, "{answer:?}");
                        // Fed back to the policy, the input still decides
                        // the same: a note is not an input.
                        assert_eq!(evaluate(&after.input).ok(), Some(before.decision.clone()));
                    }
                    let shown = assessed.first().and_then(|a| a.note);
                    if injection_suspected {
                        assert_eq!(shown, None, "suspected injection is never shown");
                    } else {
                        assert_eq!(shown, Some(answer));
                    }
                    let recorded = assessed
                        .first()
                        .and_then(|a| a.note_trace.as_ref())
                        .and_then(|t| t.note);
                    assert_eq!(recorded, Some(answer), "the trace keeps the answer");
                }
            }
        }
    }

    #[test]
    fn invalid_answers_are_dropped_and_unavailability_stops_describing() {
        let describer = Fake::new(vec![
            Err(JudgeError::Malformed),
            Err(JudgeError::NoAnswer("an unexpected model answered")),
            Ok(note(ItemKind::Logs)),
            Err(JudgeError::Unavailable("rate limit")),
            Ok(note(ItemKind::Cache)),
        ]);
        let mut assessed: Vec<Assessed> = (1..=6)
            .map(|n| {
                item(
                    n,
                    LARGE + u64::try_from(10 - n).unwrap_or(0),
                    Shape::Unknown,
                )
            })
            .collect();
        let mut progress = Vec::new();
        ready(describe_with_progress(
            &describer,
            &Epoch,
            &mut assessed,
            &request,
            &mut |done, total| progress.push((done, total)),
        ));
        assert_eq!(
            progress,
            [(0, 6), (1, 6), (2, 6), (3, 6), (4, 6)],
            "an unavailable judge never claims the remaining items are done"
        );
        assert_eq!(
            describer.asked(),
            ["item-1", "item-2", "item-3", "item-4"],
            "nothing after the judge is unavailable"
        );
        let notes: Vec<Option<ItemKind>> =
            assessed.iter().map(|a| a.note.map(|n| n.kind)).collect();
        assert_eq!(notes, [None, None, Some(ItemKind::Logs), None, None, None]);
        let validations: Vec<Option<TraceValidation>> = assessed
            .iter()
            .map(|a| a.note_trace.as_ref().map(|t| t.validation))
            .collect();
        assert_eq!(
            validations,
            [
                Some(TraceValidation::Unavailable),
                Some(TraceValidation::Unavailable),
                Some(TraceValidation::Accepted),
                Some(TraceValidation::Unavailable),
                None,
                None
            ]
        );
    }

    #[test]
    fn at_most_twenty_of_the_largest_are_described() {
        let mut assessed: Vec<Assessed> = (1..=25)
            .map(|n| item(n, LARGE * u64::try_from(n).unwrap_or(1), Shape::Unknown))
            .collect();
        assert_eq!(assessed.iter().filter(|a| describable(a)).count(), 25);
        let describer = Fake::new(vec![Ok(note(ItemKind::Unknown))]);
        ready(describe(&describer, &Epoch, &mut assessed, &request));
        let asked = describer.asked();
        assert_eq!(asked.len(), MAX_DESCRIBED);
        assert_eq!(asked.first().map(String::as_str), Some("item-25"));
        assert_eq!(asked.last().map(String::as_str), Some("item-6"));
        let undescribed: Vec<u128> = assessed
            .iter()
            .filter(|a| a.note_trace.is_none())
            .map(|a| a.candidate.identity().file.get())
            .collect();
        assert_eq!(undescribed, [1, 2, 3, 4, 5], "the five smallest");
    }
}
