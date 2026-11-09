//! Explaining installed software (ADR-0061).
//!
//! While AI review is on, the explainer is asked what each piece of installed
//! software is mainly for and what removing it could affect, at most
//! [`MAX_EXPLAINED`] per run. The answer is a note for people: nothing here
//! selects software to remove, and the policy, plans and executor never read
//! it.
//!
//! Software already explained by the same judge and prompt (the caller's
//! cache) is skipped; rows the list shows by default go first. Requests go one
//! at a time; the first answer that says the judge is unavailable or disabled
//! ends the run, and so does AI review being turned off between requests.
//! Every request is traced (`jev.trace/1` with output schema
//! `jev.software-note/2`), accepted or not.

use lumen_domain::{
    EvidenceHash, JevEffect, JevTrace, JudgeDescriptor, SchemaVersion, SoftwareNote,
    SoftwareRequest, TraceValidation,
};

use crate::consult::millis;
use crate::ports::{Clock, JudgeError, SoftwareExplainer};

/// The most pieces of software explained in one run.
pub const MAX_EXPLAINED: usize = 40;

/// One piece of software that may be explained.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SoftwareSubject<K> {
    /// The caller's cache key: origin, identifier and version.
    pub key: K,
    /// What would be sent.
    pub request: SoftwareRequest,
    /// Whether the list shows it by default (installed on request or not
    /// known otherwise, and not hidden); these are explained first.
    pub preferred: bool,
}

/// The outcome of one request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Explained<K> {
    /// The subject's key.
    pub key: K,
    /// The accepted note, also when it may not be shown
    /// ([`SoftwareNote::shown`]), so it is not asked for again; `None` when the
    /// answer was rejected or missing.
    pub note: Option<SoftwareNote>,
    /// The request's trace, for the ledger.
    pub trace: JevTrace,
}

/// The positions of the subjects to explain: those `cached` does not hold,
/// preferred ones first (otherwise in the given order), at most
/// [`MAX_EXPLAINED`].
pub fn chosen<K>(subjects: &[SoftwareSubject<K>], cached: &dyn Fn(&K) -> bool) -> Vec<usize> {
    let mut chosen: Vec<usize> = subjects
        .iter()
        .enumerate()
        .filter(|(_, s)| !cached(&s.key))
        .map(|(i, _)| i)
        .collect();
    chosen.sort_by_key(|&i| subjects.get(i).is_none_or(|s| !s.preferred));
    chosen.truncate(MAX_EXPLAINED);
    chosen
}

/// Asks `explainer` about the chosen `subjects`, one at a time, while
/// `still_on` says AI review is on. `cached` says whether the explainer's
/// current judge (`descriptor`) already explained a key. With AI review off
/// (no descriptor), nothing is asked.
pub async fn explain<K: Clone>(
    explainer: &dyn SoftwareExplainer,
    clock: &dyn Clock,
    subjects: &[SoftwareSubject<K>],
    cached: &dyn Fn(&K, &JudgeDescriptor) -> bool,
    still_on: &dyn Fn() -> bool,
) -> Vec<Explained<K>> {
    let Some(descriptor) = explainer.descriptor() else {
        return Vec::new();
    };
    let output_schema = SchemaVersion::new("jev.software-note", 2, 0)
        .unwrap_or_else(|e| unreachable!("constant schema version is valid: {e}"));
    let mut outcomes = Vec::new();
    for index in chosen(subjects, &|key| cached(key, &descriptor)) {
        let Some(subject) = subjects.get(index) else {
            continue;
        };
        if !still_on() {
            break;
        }
        let requested_at = clock.now();
        let answer = explainer.explain_software(&subject.request).await;
        let latency_ms = millis(requested_at, clock.now());
        let stop = matches!(
            answer.result,
            Err(JudgeError::Unavailable(_) | JudgeError::Disabled)
        );
        let (validation, note) = match answer.result {
            // Answers that contradict what is known become unknown (guard).
            Ok(note) => (
                TraceValidation::Accepted,
                Some(note.guarded(subject.request.protected())),
            ),
            Err(JudgeError::Rejected(reason)) => (TraceValidation::Rejected(reason), None),
            Err(_) => (TraceValidation::Unavailable, None),
        };
        let request_hash = subject.request.hash();
        outcomes.push(Explained {
            key: subject.key.clone(),
            note,
            trace: JevTrace {
                requested_at,
                judge: descriptor.clone(),
                policy_version: lumen_policy::POLICY_VERSION,
                input_schema: subject.request.schema_version().clone(),
                output_schema: output_schema.clone(),
                // No evidence bundle: the request itself is what was sent.
                evidence_hash: EvidenceHash::from_bytes(*request_hash.as_bytes()),
                request_hash,
                response_hash: answer.response_hash,
                validation,
                judgment: None,
                calibrated_precision_bp: None,
                policy_effect: JevEffect::None,
                latency_ms,
                // The software note lives in the caller's cache; the trace
                // keeps the hashes of what was sent and received.
                note: None,
            },
        });
        if stop {
            break;
        }
    }
    outcomes
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;
    use std::task::{Context as TaskContext, Poll, Waker};

    use lumen_domain::{
        JudgmentConfidence, ModelId, PayloadHash, RemovalImpact, SoftwareFacts, SoftwareOrigin,
        SoftwarePurpose, SourceName, Timestamp,
    };

    use super::*;
    use crate::ports::{Answer, NoopJudge, SoftwareNoteFuture};

    /// Answers every request with `answers` in turn (the last one repeats),
    /// and records the identifiers asked about.
    struct Fake {
        answers: Vec<Answer<SoftwareNote>>,
        asked: Mutex<Vec<String>>,
    }

    impl Fake {
        fn new(answers: Vec<Result<SoftwareNote, JudgeError>>) -> Self {
            Self {
                answers: answers.into_iter().map(Answer::from).collect(),
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

    impl SoftwareExplainer for Fake {
        fn descriptor(&self) -> Option<JudgeDescriptor> {
            Some(JudgeDescriptor {
                provider: SourceName::new("fake").unwrap_or_else(|_| unreachable!()),
                model: ModelId::new("fake-1").unwrap_or_else(|_| unreachable!()),
                model_version: None,
                prompt_version: "jev-software/1".parse().unwrap_or_else(|_| unreachable!()),
            })
        }

        fn explain_software<'a>(&'a self, request: &'a SoftwareRequest) -> SoftwareNoteFuture<'a> {
            let mut asked = self
                .asked
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let answer = self
                .answers
                .get(asked.len())
                .or(self.answers.last())
                .cloned()
                .unwrap_or_else(|| Err(JudgeError::Malformed).into());
            let id = request
                .untrusted_strings()
                .get(&lumen_domain::SoftwareField::Id)
                .map(|t| t.raw().to_owned())
                .unwrap_or_default();
            asked.push(id);
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

    fn subject(id: &str, preferred: bool) -> SoftwareSubject<String> {
        let request = SoftwareRequest::new(SoftwareFacts {
            origin: SoftwareOrigin::Homebrew,
            id,
            name: None,
            publisher: None,
            version: Some("1.0"),
            protected: false,
            hidden: false,
            services: Vec::new(),
        })
        .unwrap_or_else(|e| unreachable!("{e}"));
        SoftwareSubject {
            key: id.to_owned(),
            request,
            preferred,
        }
    }

    fn note(injection_suspected: bool) -> SoftwareNote {
        SoftwareNote {
            purpose: SoftwarePurpose::DevelopmentTool,
            removal_impact: RemovalImpact::OnlyThisApp,
            purpose_confidence: JudgmentConfidence::Medium,
            removal_impact_confidence: JudgmentConfidence::Medium,
            injection_suspected,
        }
    }

    fn none(_: &String, _: &JudgeDescriptor) -> bool {
        false
    }

    fn on() -> bool {
        true
    }

    #[test]
    fn nothing_is_asked_while_ai_review_is_off() {
        let subjects = vec![subject("a", true)];
        assert_eq!(
            ready(explain(&NoopJudge, &Epoch, &subjects, &none, &on)),
            Vec::new()
        );
        let fake = Fake::new(vec![Ok(note(false))]);
        assert_eq!(
            ready(explain(&fake, &Epoch, &subjects, &none, &|| false)),
            Vec::new(),
            "turned off before the request started"
        );
        assert_eq!(fake.asked(), Vec::<String>::new());
    }

    #[test]
    fn cached_keys_are_skipped_and_preferred_rows_go_first() {
        let subjects = vec![
            subject("dep", false),
            subject("cached", true),
            subject("app", true),
        ];
        let fake = Fake::new(vec![Ok(note(false))]);
        let explained = ready(explain(
            &fake,
            &Epoch,
            &subjects,
            &|key, judge| key == "cached" && judge.prompt_version.to_string() == "jev-software/1",
            &on,
        ));
        assert_eq!(fake.asked(), ["app", "dep"]);
        let trace = &explained[0].trace;
        assert_eq!(
            (
                trace.output_schema.to_string(),
                trace.input_schema.to_string(),
                trace.validation,
                trace.policy_effect,
            ),
            (
                "jev.software-note/2.0".to_owned(),
                "jev.software-request/1.0".to_owned(),
                TraceValidation::Accepted,
                JevEffect::None
            )
        );
        assert_eq!(trace.request_hash, subjects[2].request.hash());
        assert_eq!(trace.judgment, None);
    }

    #[test]
    fn at_most_forty_are_explained() {
        let subjects: Vec<_> = (0..50)
            .map(|i| subject(&format!("s{i}"), i >= 45))
            .collect();
        let fake = Fake::new(vec![Ok(note(false))]);
        let explained = ready(explain(&fake, &Epoch, &subjects, &none, &on));
        assert_eq!(explained.len(), MAX_EXPLAINED);
        let asked = fake.asked();
        assert_eq!(asked.first().map(String::as_str), Some("s45"));
        assert_eq!(asked.get(5).map(String::as_str), Some("s0"));
    }

    #[test]
    fn unavailability_stops_the_run_and_every_request_is_traced() {
        let subjects: Vec<_> = (0..5).map(|i| subject(&format!("s{i}"), true)).collect();
        let fake = Fake::new(vec![
            Err(JudgeError::Malformed),
            Ok(note(true)),
            Ok(note(false)),
            Err(JudgeError::Unavailable("rate limit")),
            Ok(note(false)),
        ]);
        let explained = ready(explain(&fake, &Epoch, &subjects, &none, &on));
        assert_eq!(fake.asked(), ["s0", "s1", "s2", "s3"]);
        let outcomes: Vec<_> = explained
            .iter()
            .map(|e| (e.trace.validation, e.note.map(|n| n.shown())))
            .collect();
        assert_eq!(
            outcomes,
            [
                (TraceValidation::Unavailable, None),
                (TraceValidation::Accepted, Some(false)),
                (TraceValidation::Accepted, Some(true)),
                (TraceValidation::Unavailable, None),
            ],
            "a suspected injection is kept, never as shown"
        );
        let disabled = Fake::new(vec![Err(JudgeError::Disabled)]);
        assert_eq!(
            ready(explain(&disabled, &Epoch, &subjects, &none, &on)).len(),
            1
        );
    }

    #[test]
    fn the_trace_keeps_the_hash_of_the_body_as_received() {
        let body = br#"{"purpose":"game"}"#;
        let fake = Fake {
            answers: vec![Answer::received(body, Ok(note(false)))],
            asked: Mutex::new(Vec::new()),
        };
        let explained = ready(explain(&fake, &Epoch, &[subject("a", true)], &none, &on));
        let hashes: Vec<Option<PayloadHash>> =
            explained.iter().map(|e| e.trace.response_hash).collect();
        assert_eq!(hashes, [Some(lumen_domain::response_hash(body))]);
    }
}
