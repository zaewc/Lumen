//! Recording a cassette from a live judge (docs/ai/harness.md, roadmap 12.1b).
//!
//! Each golden and adversarial case's request is sent once, one at a time,
//! and the answer body is kept exactly as received, keyed by the request hash
//! a replay looks up. Answers the validator refuses are kept too, so a replay
//! refuses them the same way; a case with no answer after the retries is left
//! out, never filled in. The recorder never sees the API key: the live judge
//! holds it.

use std::collections::BTreeMap;

use std::time::Duration;

use lumen_application::ports::JudgeError;
use lumen_domain::{JevRequest, JudgeDescriptor, PayloadHash};
use lumen_jev::openrouter::AskError;

use crate::cassette::{CASSETTE_SCHEMA, Cassette, Recorded, RecordedStop, validate_recorded};
use crate::dataset::Dataset;

/// A live judge to record from.
pub trait LiveJudge {
    /// The judge being recorded; it becomes the cassette's descriptor.
    fn descriptor(&self) -> JudgeDescriptor;

    /// Sends `request` once and returns the answer body exactly as received.
    ///
    /// # Errors
    ///
    /// An [`AskError`] when no answer arrived.
    fn ask(&self, request: &JevRequest) -> Result<String, AskError>;

    /// Waits for `pause` (between requests and before a retry).
    fn wait(&self, pause: Duration);
}

/// How requests are paced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pacing {
    /// Wait between two requests.
    pub pause: Duration,
    /// Tries per case, at least one.
    pub attempts: u32,
    /// Wait before the first retry; it doubles for each further retry.
    pub backoff: Duration,
}

impl Pacing {
    /// Jeb (ADR-0036): one request at a time, 1 s apart; a rate limit, server
    /// error or lost connection is retried up to three times, after 5, 10 and
    /// 20 s.
    pub const JEB: Self = Self {
        pause: Duration::from_secs(1),
        attempts: 4,
        backoff: Duration::from_secs(5),
    };
}

/// What happened to one case.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecordOutcome {
    /// Answered with a valid judgment; recorded.
    Recorded,
    /// Answered, but the validator refuses the answer; recorded, so a replay
    /// refuses it too.
    Refused(JudgeError),
    /// No answer after every try; not recorded.
    Failed(AskError),
    /// Not sent, because the server had rejected the key.
    NotSent,
    /// Taken unchanged from the recording being resumed; not sent again.
    Kept,
}

impl RecordOutcome {
    /// Whether the case is in the cassette.
    pub const fn is_recorded(&self) -> bool {
        matches!(self, Self::Recorded | Self::Refused(_) | Self::Kept)
    }
}

/// A finished recording.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recording {
    /// The recorded answers.
    pub cassette: Cassette,
    /// Every case with its outcome, in dataset order.
    pub outcomes: Vec<(String, RecordOutcome)>,
}

impl Recording {
    /// Whether every case was recorded.
    pub fn complete(&self) -> bool {
        self.outcomes.iter().all(|(_, o)| o.is_recorded())
    }
}

/// The model an answer names, or nothing.
fn reported_model(body: &str) -> String {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|v| v.get("model")?.as_str().map(str::to_owned))
        .unwrap_or_default()
}

fn ask_with_retries(
    judge: &dyn LiveJudge,
    request: &JevRequest,
    pacing: Pacing,
) -> Result<String, AskError> {
    let mut backoff = pacing.backoff;
    let mut tries = 1;
    loop {
        match judge.ask(request) {
            Err(error) if error.is_transient() && tries < pacing.attempts => {
                judge.wait(backoff);
                backoff = backoff.saturating_mul(2);
                tries += 1;
            }
            answer => return answer,
        }
    }
}

/// Records `judge`'s answers to every golden and adversarial case of
/// `dataset`, sequentially, reporting each case to `progress` as it is done.
/// A case `previous` already holds for the same request is kept, not sent
/// again, so an interrupted recording can be completed; the caller checks that
/// `previous` came from the same judge. After the server rejects the key, the
/// remaining cases are not sent.
pub fn record(
    judge: &dyn LiveJudge,
    dataset: &Dataset,
    pacing: Pacing,
    previous: Option<&Cassette>,
    progress: &mut dyn FnMut(&str, &RecordOutcome),
) -> Recording {
    let descriptor = judge.descriptor();
    let kept: BTreeMap<(&str, PayloadHash), &Recorded> = previous
        .map(|cassette| {
            cassette
                .entries
                .iter()
                .map(|e| ((e.case_id.as_str(), e.request_hash), e))
                .collect()
        })
        .unwrap_or_default();
    let mut entries = Vec::new();
    let mut outcomes = Vec::new();
    let mut key_rejected = false;
    let mut sent = false;
    for case in dataset.golden.iter().chain(&dataset.adversarial) {
        let outcome = if let Some(entry) = kept.get(&(case.case_id.as_str(), case.request.hash())) {
            entries.push((*entry).clone());
            RecordOutcome::Kept
        } else if key_rejected {
            RecordOutcome::NotSent
        } else {
            if sent {
                judge.wait(pacing.pause);
            }
            sent = true;
            match ask_with_retries(judge, &case.request, pacing) {
                Ok(body) => {
                    let recorded = Recorded {
                        case_id: case.case_id.clone(),
                        request_hash: case.request.hash(),
                        stop_reason: RecordedStop::EndTurn,
                        model: reported_model(&body),
                        body,
                    };
                    let outcome = match validate_recorded(&descriptor, &case.request, &recorded) {
                        Ok(_) => RecordOutcome::Recorded,
                        Err(error) => RecordOutcome::Refused(error),
                    };
                    entries.push(recorded);
                    outcome
                }
                Err(error) => {
                    key_rejected = error.is_key_rejected();
                    RecordOutcome::Failed(error)
                }
            }
        };
        progress(&case.case_id, &outcome);
        outcomes.push((case.case_id.clone(), outcome));
    }
    Recording {
        cassette: Cassette {
            schema: CASSETTE_SCHEMA.to_owned(),
            judge: descriptor,
            entries,
        },
        outcomes,
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::BTreeMap;
    use std::path::Path;

    use lumen_application::ports::JudgeModel as _;
    use lumen_domain::{Assessment, ModelId, PromptVersion, SourceName};
    use lumen_jev::decisions::{PROMPT_VERSION, Provider};

    use super::*;
    use crate::{CassetteError, Grade, ReplayJudge, load, run_dataset};

    const KEY: &str = "jeb-secret-key-123";

    /// A scripted Jeb: per item, the HTTP results it gives in turn (the last
    /// repeats); otherwise a valid answer with the case's expected assessment.
    struct FakeJeb {
        key: String,
        expected: BTreeMap<String, Assessment>,
        script: BTreeMap<String, Vec<Result<String, AskError>>>,
        sent: RefCell<Vec<String>>,
        waits: RefCell<Vec<Duration>>,
    }

    fn answer(assessment: Assessment, model: &str) -> String {
        let choice = serde_json::to_value(assessment).unwrap_or_default();
        // A reason that fits the assessment, so the evidence guards keep it.
        let reason = if assessment == Assessment::LikelyRemovable {
            "stale_by_time"
        } else {
            "insufficient_evidence"
        };
        serde_json::json!({
            "answers": {
                "assessment": {"type": "choice", "choice": choice, "confidence": 0.95,
                    "probabilities": {choice.as_str().unwrap_or_default(): 0.95}},
                "reason": {"type": "choice", "choice": reason,
                    "probabilities": {reason: 1.0}},
                "injection": {"type": "noul", "noul": 0.0},
            },
            "model": model,
            "usage": {"input_tokens": 500, "output_tokens": 40},
        })
        .to_string()
    }

    impl LiveJudge for FakeJeb {
        fn descriptor(&self) -> JudgeDescriptor {
            let (name, revision) = PROMPT_VERSION.split_once('/').unwrap_or_default();
            JudgeDescriptor {
                provider: SourceName::new(Provider::Jeb.source())
                    .unwrap_or_else(|_| unreachable!()),
                model: ModelId::new(Provider::Jeb.model()).unwrap_or_else(|_| unreachable!()),
                model_version: None,
                prompt_version: PromptVersion::new(name, revision.parse().unwrap_or(0))
                    .unwrap_or_else(|_| unreachable!()),
            }
        }

        fn ask(&self, request: &JevRequest) -> Result<String, AskError> {
            // The judge, not the recorder, holds and checks the key.
            if self.key != KEY {
                return Err(AskError {
                    status: Some(401),
                    reason: "rejected",
                });
            }
            let item = request.item_ref.as_str().to_owned();
            let tries = self.sent.borrow().iter().filter(|i| **i == item).count();
            self.sent.borrow_mut().push(item.clone());
            if let Some(script) = self.script.get(&item) {
                return script
                    .get(tries)
                    .or(script.last())
                    .cloned()
                    .unwrap_or_else(|| unreachable!());
            }
            let expected = self.expected.get(&item).copied();
            Ok(answer(
                expected.unwrap_or(Assessment::Uncertain),
                "jeb-20261007",
            ))
        }

        fn wait(&self, pause: Duration) {
            self.waits.borrow_mut().push(pause);
        }
    }

    fn dataset() -> Dataset {
        let ai = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ai");
        load(&ai, "2026.10.0").unwrap_or_else(|e| unreachable!("{e}"))
    }

    fn fake(dataset: &Dataset, script: &[(&str, Vec<Result<String, AskError>>)]) -> FakeJeb {
        let all = dataset.golden.iter().chain(&dataset.adversarial);
        FakeJeb {
            key: KEY.to_owned(),
            expected: all
                .map(|c| {
                    (
                        c.request.item_ref.as_str().to_owned(),
                        c.expected.assessment,
                    )
                })
                .collect(),
            script: script
                .iter()
                .map(|(item, results)| ((*item).to_owned(), results.clone()))
                .collect(),
            sent: RefCell::new(Vec::new()),
            waits: RefCell::new(Vec::new()),
        }
    }

    fn item(dataset: &Dataset, index: usize) -> String {
        dataset.golden.get(index).map_or_else(
            || unreachable!(),
            |c| c.request.item_ref.as_str().to_owned(),
        )
    }

    const BUSY: AskError = AskError {
        status: Some(429),
        reason: "busy",
    };
    const DOWN: AskError = AskError {
        status: Some(503),
        reason: "down",
    };

    #[test]
    fn resuming_sends_only_the_missing_cases() {
        let dataset = dataset();
        let failed = item(&dataset, 2);
        let first = record(
            &fake(&dataset, &[(&failed, vec![Err(DOWN)])]),
            &dataset,
            Pacing::JEB,
            None,
            &mut |_, _| {},
        );
        assert!(!first.complete());
        let judge = fake(&dataset, &[]);
        let resumed = record(
            &judge,
            &dataset,
            Pacing::JEB,
            Some(&first.cassette),
            &mut |_, _| {},
        );
        // Only the case that failed is asked again, with no pause before it.
        assert_eq!(*judge.sent.borrow(), [failed]);
        assert!(judge.waits.borrow().is_empty());
        assert!(resumed.complete());
        let kept = resumed
            .outcomes
            .iter()
            .filter(|(_, o)| *o == RecordOutcome::Kept)
            .count();
        assert_eq!(kept, first.cassette.entries.len());
        // Entries stay in dataset order, the kept ones unchanged.
        let order: Vec<&str> = resumed
            .cassette
            .entries
            .iter()
            .map(|e| e.case_id.as_str())
            .collect();
        let expected: Vec<&str> = dataset
            .golden
            .iter()
            .chain(&dataset.adversarial)
            .map(|c| c.case_id.as_str())
            .collect();
        assert_eq!(order, expected);
        for entry in &first.cassette.entries {
            assert!(resumed.cassette.entries.contains(entry));
        }
    }

    #[test]
    fn a_recording_replays_through_run_to_the_same_grades() -> Result<(), CassetteError> {
        let dataset = dataset();
        let (retried, refused, failed) = (item(&dataset, 0), item(&dataset, 1), item(&dataset, 2));
        let late = answer(dataset.golden[0].expected.assessment, "jeb");
        let judge = fake(
            &dataset,
            &[
                (&retried, vec![Err(BUSY), Err(DOWN), Ok(late)]),
                (
                    &refused,
                    vec![Ok(answer(Assessment::LikelyNeeded, "gpt-5"))],
                ),
                (&failed, vec![Err(DOWN)]),
            ],
        );
        let mut lines = Vec::new();
        let recording = record(&judge, &dataset, Pacing::JEB, None, &mut |id, outcome| {
            lines.push((id.to_owned(), outcome.clone()));
        });
        let cases = dataset.golden.len() + dataset.adversarial.len();
        assert_eq!(lines, recording.outcomes, "one progress line per case");
        assert_eq!(recording.outcomes.len(), cases);
        assert!(!recording.complete());
        assert_eq!(
            recording.cassette.entries.len(),
            cases - 1,
            "the failure is left out"
        );
        assert_eq!(recording.cassette.judge, judge.descriptor());

        // Sequential, paced, bounded: 3 tries for the retried case, 4 for the
        // failed one, one each for the rest.
        assert_eq!(judge.sent.borrow().len(), cases + 2 + 3);
        let waits = judge.waits.borrow();
        let pause = waits.iter().filter(|w| **w == Pacing::JEB.pause).count();
        assert_eq!(pause, cases - 1);
        let backoffs: Vec<u64> = waits
            .iter()
            .filter(|w| **w != Pacing::JEB.pause)
            .map(Duration::as_secs)
            .collect();
        assert_eq!(backoffs, [5, 10, 5, 10, 20]);

        // Keyed by the request hashes `run` looks up, with the body unchanged.
        for entry in &recording.cassette.entries {
            let case = dataset
                .golden
                .iter()
                .chain(&dataset.adversarial)
                .find(|c| c.case_id == entry.case_id)
                .unwrap_or_else(|| unreachable!());
            assert_eq!(entry.request_hash, case.request.hash());
            assert_eq!(entry.stop_reason, RecordedStop::EndTurn);
            assert!(
                entry
                    .body
                    .contains(&format!("\"model\":\"{}\"", entry.model))
            );
        }

        // The written cassette carries no key or header, and replays.
        let text = serde_json::to_string_pretty(&recording.cassette)?;
        for marker in [KEY, "Bearer", "Authorization"] {
            assert!(!text.contains(marker));
        }
        let replay = ReplayJudge::new(Cassette::parse(&text)?);
        let report = run_dataset(&replay, &dataset);
        for (outcome, (case_id, recorded)) in report
            .golden
            .outcomes
            .iter()
            .chain(&report.adversarial.outcomes)
            .zip(&recording.outcomes)
        {
            assert_eq!(&outcome.case_id, case_id);
            let expected = match recorded {
                RecordOutcome::Recorded => Grade::Correct,
                _ => Grade::NoAnswer,
            };
            assert_eq!(outcome.grade, expected, "{case_id}: {recorded:?}");
        }
        let refused_case = &dataset.golden[1].case_id;
        assert!(recording.outcomes.iter().any(|(id, o)| id == refused_case
            && matches!(o, RecordOutcome::Refused(JudgeError::NoAnswer(_)))));
        // The replay answers exactly as the recorder's validation did.
        assert_eq!(
            crate::block_on(replay.judge(&dataset.golden[1].request)),
            Err(JudgeError::NoAnswer("an unexpected model answered"))
        );
        Ok(())
    }

    #[test]
    fn a_rejected_key_stops_the_recording() {
        let dataset = dataset();
        let first = item(&dataset, 0);
        let rejected = AskError {
            status: Some(401),
            reason: "rejected",
        };
        let judge = fake(&dataset, &[(&first, vec![Err(rejected)])]);
        let recording = record(&judge, &dataset, Pacing::JEB, None, &mut |_, _| {});
        assert_eq!(
            judge.sent.borrow().len(),
            1,
            "not retried, nothing else sent"
        );
        assert_eq!(recording.cassette.entries, []);
        assert_eq!(
            recording.outcomes.first().map(|(_, o)| o),
            Some(&RecordOutcome::Failed(rejected))
        );
        assert!(
            recording
                .outcomes
                .iter()
                .skip(1)
                .all(|(_, o)| *o == RecordOutcome::NotSent)
        );
    }
}
