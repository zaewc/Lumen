//! Cassettes (`jev.cassette/1`) and replaying them.
//!
//! A replay validates each recorded body on the path the recorded judge uses
//! live: a Decisions-protocol judge (Jeb, or Jev through `OpenRouter`, named
//! by its descriptor's provider) through [`decisions::judgment_for`] with that
//! provider's pinned model and the prompt version the descriptor records, any
//! other judge through [`validate`]. A Decisions cassette names one of the
//! known prompt versions ([`Prompt`]), or it is not a cassette this code can
//! replay. Answers of Apple's on-device model (provider `apple.fm`, ADR-0044)
//! go through [`foundation::judgment_for`], and must come from the model
//! version (OS build) the descriptor records. Answers of the AI Hub's chat
//! model (provider `gsm.chat`, ADR-0045) go through [`chat::judgment_for`].

use std::collections::BTreeMap;

use lumen_application::ports::{JudgeError, JudgeFuture, JudgeModel};
use lumen_domain::{JevRequest, JudgeDescriptor, Judgment, PayloadHash, UntrustedText};
use lumen_jev::decisions::{self, Prompt, Provider};
use lumen_jev::response::{RawResponse, StopReason, validate};
use lumen_jev::{chat, foundation};
use serde::{Deserialize, Serialize};

/// The cassette format this code reads and writes.
pub const CASSETTE_SCHEMA: &str = "jev.cassette/1";

/// Why the model stopped, as recorded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecordedStop {
    /// It finished its answer.
    EndTurn,
    /// It ran out of output tokens.
    MaxTokens,
    /// It declined to answer.
    Refusal,
    /// Anything else.
    Other,
}

impl From<RecordedStop> for StopReason {
    fn from(stop: RecordedStop) -> Self {
        match stop {
            RecordedStop::EndTurn => Self::EndTurn,
            RecordedStop::MaxTokens => Self::MaxTokens,
            RecordedStop::Refusal => Self::Refusal,
            RecordedStop::Other => Self::Other,
        }
    }
}

/// One recorded answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Recorded {
    /// The evaluation case it belongs to.
    pub case_id: String,
    /// Hash of the exact request it answered.
    pub request_hash: PayloadHash,
    /// Why the model stopped.
    pub stop_reason: RecordedStop,
    /// The model that answered, as the provider reported it.
    pub model: String,
    /// The raw answer body.
    pub body: String,
}

/// Recorded answers from one judge configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cassette {
    /// Always [`CASSETTE_SCHEMA`].
    pub schema: String,
    /// The judge that was recorded (provider, pinned model, prompt version).
    pub judge: JudgeDescriptor,
    /// The answers.
    pub entries: Vec<Recorded>,
}

/// Why a cassette cannot be used.
#[derive(Debug, thiserror::Error)]
pub enum CassetteError {
    /// It is not valid cassette JSON.
    #[error("not a cassette: {0}")]
    Parse(#[from] serde_json::Error),
    /// It is another format or version.
    #[error("unsupported cassette schema `{0}`")]
    Schema(String),
    /// Two entries answer the same request.
    #[error("two recordings answer the same request")]
    Duplicate,
    /// A recording to resume comes from another judge, model or prompt.
    #[error("the recording to resume comes from another judge, model or prompt")]
    OtherJudge,
    /// A Decisions judge with a prompt version this code does not know.
    #[error("unknown prompt version `{0}` for a Decisions judge")]
    UnknownPrompt(String),
    /// The cassette was recorded with another prompt than the one expected.
    #[error("the cassette was recorded with prompt `{recorded}`, not `{expected}`")]
    OtherPrompt {
        /// The prompt version the cassette records.
        recorded: String,
        /// The prompt version expected.
        expected: &'static str,
    },
}

impl Cassette {
    /// Parses and checks a cassette.
    ///
    /// # Errors
    ///
    /// A [`CassetteError`] for invalid JSON, another schema, or duplicate
    /// requests.
    pub fn parse(text: &str) -> Result<Self, CassetteError> {
        let cassette: Self = serde_json::from_str(text)?;
        if cassette.schema != CASSETTE_SCHEMA {
            return Err(CassetteError::Schema(cassette.schema));
        }
        let mut seen = std::collections::BTreeSet::new();
        if !cassette.entries.iter().all(|e| seen.insert(e.request_hash)) {
            return Err(CassetteError::Duplicate);
        }
        let prompt = cassette.judge.prompt_version.to_string();
        if (decisions_provider(&cassette.judge).is_some() && cassette.prompt().is_none())
            || (is_on_device(&cassette.judge) && prompt != foundation::PROMPT_VERSION)
            || (is_chat(&cassette.judge) && prompt != chat::PROMPT_VERSION)
        {
            return Err(CassetteError::UnknownPrompt(prompt));
        }
        Ok(cassette)
    }

    /// The Decisions prompt the answers were recorded with, or `None` for a
    /// judge answering `jev.judgment/1` text.
    pub fn prompt(&self) -> Option<Prompt> {
        decisions_prompt(&self.judge)
    }

    /// Checks that the answers were recorded with `expected`.
    ///
    /// # Errors
    ///
    /// [`CassetteError::OtherPrompt`] if another prompt (or no Decisions
    /// prompt) was recorded.
    pub fn expect_prompt(&self, expected: Prompt) -> Result<(), CassetteError> {
        if self.prompt() == Some(expected) {
            Ok(())
        } else {
            Err(CassetteError::OtherPrompt {
                recorded: self.judge.prompt_version.to_string(),
                expected: expected.version(),
            })
        }
    }
}

/// Answers requests from a cassette, validating every answer exactly as a live
/// judge would. Requests that were not recorded are unavailable.
#[derive(Debug, Clone)]
pub struct ReplayJudge {
    judge: JudgeDescriptor,
    answers: BTreeMap<PayloadHash, Recorded>,
}

impl ReplayJudge {
    /// A judge replaying `cassette`.
    pub fn new(cassette: Cassette) -> Self {
        Self {
            judge: cassette.judge,
            answers: cassette
                .entries
                .into_iter()
                .map(|e| (e.request_hash, e))
                .collect(),
        }
    }
}

/// The Decisions provider `judge` names, or `None` for a judge answering
/// `jev.judgment/1` text.
fn decisions_provider(judge: &JudgeDescriptor) -> Option<Provider> {
    [Provider::Jeb, Provider::OpenRouter]
        .into_iter()
        .find(|p| judge.provider.as_str() == p.source())
}

/// Whether `judge` is Apple's on-device model (ADR-0044).
fn is_on_device(judge: &JudgeDescriptor) -> bool {
    judge.provider.as_str() == foundation::PROVIDER
}

/// Whether `judge` is the AI Hub's chat model (ADR-0045).
fn is_chat(judge: &JudgeDescriptor) -> bool {
    judge.provider.as_str() == chat::PROVIDER
}

/// The Decisions prompt `judge` names, if it is a Decisions judge with a
/// known prompt version.
fn decisions_prompt(judge: &JudgeDescriptor) -> Option<Prompt> {
    decisions_provider(judge)?;
    Prompt::from_version(&judge.prompt_version.to_string())
}

/// Validates `recorded` as the answer to `request` exactly as `judge` would
/// live, with the prompt version `judge` names.
pub(crate) fn validate_recorded(
    judge: &JudgeDescriptor,
    request: &JevRequest,
    recorded: &Recorded,
) -> Result<Judgment, JudgeError> {
    if is_on_device(judge) {
        return validate_on_device(judge, request, recorded);
    }
    if is_chat(judge) {
        return validate_chat(judge, request, recorded);
    }
    let Some(provider) = decisions_provider(judge) else {
        return validate(
            request,
            &judge.model,
            &RawResponse {
                stop_reason: recorded.stop_reason.into(),
                model: &recorded.model,
                body: &recorded.body,
            },
        );
    };
    // Decisions answers carry no stop reason: live, only a complete HTTP
    // answer reaches the validator, and it is recorded as `end_turn`.
    if recorded.stop_reason != RecordedStop::EndTurn {
        return Err(JudgeError::NoAnswer(
            "the recording is not a complete answer",
        ));
    }
    let prompt = decisions_prompt(judge).ok_or(JudgeError::NoAnswer(
        "the recording names an unknown prompt",
    ))?;
    let (judgment, model) = decisions::judgment_for(request, &recorded.body, provider, prompt)?;
    if model != recorded.model {
        return Err(JudgeError::NoAnswer(
            "the recorded model differs from the answer's",
        ));
    }
    Ok(judgment)
}

/// Validates an on-device answer: complete, from the recorded model and the
/// model version (OS build) the descriptor names.
fn validate_on_device(
    judge: &JudgeDescriptor,
    request: &JevRequest,
    recorded: &Recorded,
) -> Result<Judgment, JudgeError> {
    if recorded.stop_reason != RecordedStop::EndTurn {
        return Err(JudgeError::NoAnswer(
            "the recording is not a complete answer",
        ));
    }
    let (judgment, answered) = foundation::judgment_for(request, &recorded.body)?;
    if answered.model != recorded.model || judge.model.as_str() != answered.model {
        return Err(JudgeError::NoAnswer(
            "the recorded model differs from the answer's",
        ));
    }
    if judge.model_version.as_ref().map(UntrustedText::raw) != Some(answered.model_version.as_str())
    {
        return Err(JudgeError::NoAnswer(
            "the answer comes from another model version",
        ));
    }
    Ok(judgment)
}

/// Validates a chat answer: complete, and from the pinned model, which the
/// descriptor and the recording name too.
fn validate_chat(
    judge: &JudgeDescriptor,
    request: &JevRequest,
    recorded: &Recorded,
) -> Result<Judgment, JudgeError> {
    if recorded.stop_reason != RecordedStop::EndTurn {
        return Err(JudgeError::NoAnswer(
            "the recording is not a complete answer",
        ));
    }
    let judgment = chat::judgment_for(request, &recorded.body)?;
    if recorded.model != chat::MODEL || judge.model.as_str() != chat::MODEL {
        return Err(JudgeError::NoAnswer(
            "the recorded model differs from the answer's",
        ));
    }
    Ok(judgment)
}

impl JudgeModel for ReplayJudge {
    fn descriptor(&self) -> Option<JudgeDescriptor> {
        Some(self.judge.clone())
    }

    fn judge<'a>(&'a self, request: &'a JevRequest) -> JudgeFuture<'a> {
        let answer = match self.answers.get(&request.hash()) {
            Some(recorded) => validate_recorded(&self.judge, request, recorded),
            None => Err(JudgeError::Unavailable("no recording for this request")),
        };
        Box::pin(std::future::ready(answer))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::task::{Context, Poll, Waker};

    use lumen_domain::{
        Assessment, ByteCount, CandidateSummary, Category, CoverageSummary, EvidenceId,
        EvidenceKind, ItemRef, Judgment, ModelId, RequestEvidence, UntrustedText,
    };

    use super::*;

    const MODEL: &str = "claude-sonnet-5-5";

    fn id() -> EvidenceId {
        EvidenceId::from_bytes([1; 32])
    }

    fn request(item: &str) -> JevRequest {
        JevRequest {
            schema_version: JevRequest::current_schema().unwrap_or_else(|_| unreachable!()),
            item_ref: ItemRef::new(item).unwrap_or_else(|_| unreachable!()),
            candidate: CandidateSummary {
                category: Category::AppCache,
                path_display: UntrustedText::new("~/Library/Caches/x"),
                size_bytes: ByteCount::ZERO,
                reclaimable_bytes: ByteCount::ZERO,
            },
            evidence: vec![RequestEvidence {
                id: id(),
                kind: EvidenceKind::OwningApplication,
                value: UntrustedText::new("x"),
            }],
            coverage: CoverageSummary {
                complete: true,
                notes: BTreeSet::new(),
            },
            untrusted_strings: BTreeMap::new(),
        }
    }

    fn body(item: &str) -> String {
        serde_json::json!({
            "schema_version": "jev.judgment/1",
            "item_ref": item,
            "assessment": "likely_needed",
            "confidence": "high",
            "reason_codes": ["owner_app_present"],
            "evidence_refs": [id().to_string()],
            "injection_suspected": false,
            "rationale": "Recorded.",
        })
        .to_string()
    }

    fn cassette(entries: &[Recorded]) -> String {
        serde_json::json!({
            "schema": CASSETTE_SCHEMA,
            "judge": {
                "provider": "anthropic",
                "model": MODEL,
                "model_version": null,
                "prompt_version": "jev-judge/1",
            },
            "entries": entries,
        })
        .to_string()
    }

    fn recorded(item: &str, stop_reason: RecordedStop, model: &str) -> Recorded {
        Recorded {
            case_id: format!("case-{item}"),
            request_hash: request(item).hash(),
            stop_reason,
            model: model.to_owned(),
            body: body(item),
        }
    }

    fn replay(judge: &ReplayJudge, item: &str) -> Result<Judgment, JudgeError> {
        let request = request(item);
        match judge
            .judge(&request)
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
        {
            Poll::Ready(answer) => answer,
            Poll::Pending => unreachable!("replays answer immediately"),
        }
    }

    #[test]
    fn replays_validate_like_live_answers() -> Result<(), CassetteError> {
        let judge = ReplayJudge::new(Cassette::parse(&cassette(&[
            recorded("c_1", RecordedStop::EndTurn, MODEL),
            recorded("c_2", RecordedStop::Refusal, MODEL),
            recorded("c_3", RecordedStop::EndTurn, "claude-other"),
        ]))?);
        assert_eq!(
            judge.descriptor().map(|d| d.model),
            Some(ModelId::new(MODEL).unwrap_or_else(|_| unreachable!()))
        );
        let first = replay(&judge, "c_1");
        assert_eq!(
            first.as_ref().map(|j| j.assessment),
            Ok(Assessment::LikelyNeeded)
        );
        assert_eq!(first, replay(&judge, "c_1"), "deterministic");
        assert!(matches!(
            replay(&judge, "c_2"),
            Err(JudgeError::NoAnswer(_))
        ));
        assert!(
            matches!(replay(&judge, "c_3"), Err(JudgeError::NoAnswer(_))),
            "pinned model"
        );
        assert!(matches!(
            replay(&judge, "c_9"),
            Err(JudgeError::Unavailable(_))
        ));
        Ok(())
    }

    /// A Decisions answer from `model`, as received (trailing newline kept).
    fn decision(model: &str) -> String {
        serde_json::json!({
            "answers": {
                "assessment": {"type": "choice", "choice": "likely_needed",
                    "probabilities": {"likely_needed": 0.95, "uncertain": 0.05}},
                "reason": {"type": "choice", "choice": "owner_app_present",
                    "probabilities": {"owner_app_present": 1.0}},
                "injection": {"type": "noul", "noul": 0.01},
            },
            "model": model,
            "usage": {"input_tokens": 400, "output_tokens": 60},
        })
        .to_string()
            + "\n"
    }

    #[test]
    fn jeb_recordings_replay_through_the_decisions_validator() -> Result<(), CassetteError> {
        let entry = |item: &str, stop, model: &str, body: String| Recorded {
            case_id: format!("case-{item}"),
            request_hash: request(item).hash(),
            stop_reason: stop,
            model: model.to_owned(),
            body,
        };
        let text = serde_json::json!({
            "schema": CASSETTE_SCHEMA,
            "judge": {
                "provider": "gsm.jeb",
                "model": "jeb",
                "model_version": null,
                "prompt_version": "jev-decisions/1",
            },
            "entries": [
                entry("c_1", RecordedStop::EndTurn, "jeb-20261007", decision("jeb-20261007")),
                entry("c_2", RecordedStop::EndTurn, "typesafe/jev-1.13", decision("typesafe/jev-1.13")),
                entry("c_3", RecordedStop::EndTurn, "jeb", body("c_3")),
                entry("c_4", RecordedStop::Other, "jeb", decision("jeb")),
                entry("c_5", RecordedStop::EndTurn, "jeb-20261007", decision("jeb")),
            ],
        })
        .to_string();
        let judge = ReplayJudge::new(Cassette::parse(&text)?);
        let live = decisions::judgment_for(
            &request("c_1"),
            &decision("jeb-20261007"),
            Provider::Jeb,
            Prompt::V1,
        )
        .map(|(judgment, _)| judgment);
        assert_eq!(replay(&judge, "c_1"), live, "replay == live");
        assert_eq!(
            replay(&judge, "c_1").map(|j| j.assessment),
            Ok(Assessment::LikelyNeeded)
        );
        assert!(
            matches!(replay(&judge, "c_2"), Err(JudgeError::NoAnswer(_))),
            "Jeb's pinned model only"
        );
        assert_eq!(
            replay(&judge, "c_3"),
            Err(JudgeError::Malformed),
            "text answer"
        );
        assert!(matches!(
            replay(&judge, "c_4"),
            Err(JudgeError::NoAnswer(_))
        ));
        assert!(matches!(
            replay(&judge, "c_5"),
            Err(JudgeError::NoAnswer(_))
        ));
        Ok(())
    }

    #[test]
    fn invalid_cassettes_are_rejected() {
        let ok = recorded("c_1", RecordedStop::EndTurn, MODEL);
        assert!(matches!(
            Cassette::parse(&cassette(&[ok.clone(), ok.clone()])),
            Err(CassetteError::Duplicate)
        ));
        let other = cassette(&[ok]).replace(CASSETTE_SCHEMA, "jev.cassette/2");
        assert!(matches!(
            Cassette::parse(&other),
            Err(CassetteError::Schema(_))
        ));
        assert!(matches!(
            Cassette::parse("{}"),
            Err(CassetteError::Parse(_))
        ));
        let extra = cassette(&[]).replace("\"entries\"", "\"extra\":1,\"entries\"");
        assert!(matches!(
            Cassette::parse(&extra),
            Err(CassetteError::Parse(_))
        ));
    }

    fn jeb_cassette(prompt: &str) -> String {
        serde_json::json!({
            "schema": CASSETTE_SCHEMA,
            "judge": {
                "provider": "gsm.jeb",
                "model": "jeb",
                "model_version": null,
                "prompt_version": prompt,
            },
            "entries": [Recorded {
                case_id: "case-c_1".to_owned(),
                request_hash: request("c_1").hash(),
                stop_reason: RecordedStop::EndTurn,
                model: "jeb".to_owned(),
                body: decision("jeb"),
            }],
        })
        .to_string()
    }

    #[test]
    fn a_decisions_cassette_is_pinned_to_its_prompt() -> Result<(), CassetteError> {
        for (prompt, other) in [(Prompt::V1, Prompt::V2), (Prompt::V2, Prompt::V1)] {
            let cassette = Cassette::parse(&jeb_cassette(prompt.version()))?;
            assert_eq!(cassette.prompt(), Some(prompt));
            assert!(cassette.expect_prompt(prompt).is_ok());
            assert!(
                matches!(
                    cassette.expect_prompt(other),
                    Err(CassetteError::OtherPrompt { expected, .. }) if expected == other.version()
                ),
                "a {prompt:?} cassette cannot validate as {other:?}"
            );
            let judge = ReplayJudge::new(cassette);
            // Traces name the prompt that answered.
            assert_eq!(
                judge.descriptor().map(|d| d.prompt_version.to_string()),
                Some(prompt.version().to_owned())
            );
            let live =
                decisions::judgment_for(&request("c_1"), &decision("jeb"), Provider::Jeb, prompt)
                    .map(|(judgment, _)| judgment);
            assert_eq!(replay(&judge, "c_1"), live, "replay == live, {prompt:?}");
        }
        for unknown in ["jev-decisions/3", "jev-judge/1"] {
            assert!(
                matches!(
                    Cassette::parse(&jeb_cassette(unknown)),
                    Err(CassetteError::UnknownPrompt(p)) if p == unknown
                ),
                "{unknown}"
            );
        }
        // A judge answering `jev.judgment/1` text has no Decisions prompt.
        let text = Cassette::parse(&cassette(&[]))?;
        assert_eq!(text.prompt(), None);
        assert!(text.expect_prompt(Prompt::V1).is_err());
        Ok(())
    }

    /// An on-device answer about `item` from `version`.
    fn on_device(item: &str, version: &str) -> String {
        serde_json::json!({
            "schema": "fm.answer/1",
            "item_ref": item,
            "prompt_version": "fm-decisions/1",
            "model": "apple.foundation-models",
            "model_version": version,
            "answers": {"assessment": "likely_needed", "reason": "owner_app_present",
                "injection_suspected": false},
            "elapsed_ms": 700,
        })
        .to_string()
    }

    fn on_device_cassette(prompt: &str, entries: &[Recorded]) -> String {
        serde_json::json!({
            "schema": CASSETTE_SCHEMA,
            "judge": {
                "provider": "apple.fm",
                "model": "apple.foundation-models",
                "model_version": {"text": "macOS 26.6.2 (25G83)", "truncated": false},
                "prompt_version": prompt,
            },
            "entries": entries,
        })
        .to_string()
    }

    #[test]
    fn on_device_recordings_replay_through_their_validator() -> Result<(), CassetteError> {
        const VERSION: &str = "macOS 26.6.2 (25G83)";
        let entry = |item: &str, stop, model: &str, body: String| Recorded {
            case_id: format!("case-{item}"),
            request_hash: request(item).hash(),
            stop_reason: stop,
            model: model.to_owned(),
            body,
        };
        let fm = "apple.foundation-models";
        let text = on_device_cassette(
            "fm-decisions/1",
            &[
                entry("c_1", RecordedStop::EndTurn, fm, on_device("c_1", VERSION)),
                entry(
                    "c_2",
                    RecordedStop::EndTurn,
                    fm,
                    on_device("c_2", "macOS 26.7 (25H1)"),
                ),
                entry("c_3", RecordedStop::Other, fm, on_device("c_3", VERSION)),
                entry(
                    "c_4",
                    RecordedStop::EndTurn,
                    "jeb",
                    on_device("c_4", VERSION),
                ),
                entry("c_5", RecordedStop::EndTurn, fm, decision("jeb")),
            ],
        );
        let judge = ReplayJudge::new(Cassette::parse(&text)?);
        let live = foundation::judgment_for(&request("c_1"), &on_device("c_1", VERSION))
            .map(|(judgment, _)| judgment);
        assert_eq!(replay(&judge, "c_1"), live, "replay == live");
        assert_eq!(
            replay(&judge, "c_1").map(|j| j.assessment),
            Ok(Assessment::LikelyNeeded)
        );
        for item in ["c_2", "c_3", "c_4"] {
            assert!(
                matches!(replay(&judge, item), Err(JudgeError::NoAnswer(_))),
                "{item}"
            );
        }
        assert_eq!(replay(&judge, "c_5"), Err(JudgeError::Malformed));
        assert_eq!(
            judge.descriptor().map(|d| d.provider.to_string()),
            Some("apple.fm".into())
        );
        assert_eq!(
            Cassette::parse(&text)?.prompt(),
            None,
            "not a Decisions prompt"
        );
        for other in ["fm-decisions/2", "jev-decisions/1"] {
            assert!(matches!(
                Cassette::parse(&on_device_cassette(other, &[])),
                Err(CassetteError::UnknownPrompt(p)) if p == other
            ));
        }
        Ok(())
    }

    /// A chat completion from `model` saying `content`.
    fn chat_answer(model: &str, content: &str) -> String {
        serde_json::json!({
            "model": model,
            "choices": [{"index": 0, "finish_reason": "stop",
                "message": {"role": "assistant", "content": content}}],
            "usage": {"prompt_tokens": 900, "completion_tokens": 30, "total_tokens": 930},
        })
        .to_string()
    }

    fn chat_cassette(prompt: &str, entries: &[Recorded]) -> String {
        serde_json::json!({
            "schema": CASSETTE_SCHEMA,
            "judge": {"provider": "gsm.chat", "model": "flash-next", "prompt_version": prompt},
            "entries": entries,
        })
        .to_string()
    }

    #[test]
    fn chat_recordings_replay_through_their_validator() -> Result<(), CassetteError> {
        const NEEDED: &str = r#"{"injection_suspected": false, "reason": "owner_app_present", "assessment": "likely_needed"}"#;
        let entry = |item: &str, stop, model: &str, body: String| Recorded {
            case_id: format!("case-{item}"),
            request_hash: request(item).hash(),
            stop_reason: stop,
            model: model.to_owned(),
            body,
        };
        let flash = "flash-next";
        let text = chat_cassette(
            "chat-decisions/1",
            &[
                entry(
                    "c_1",
                    RecordedStop::EndTurn,
                    flash,
                    chat_answer(flash, NEEDED),
                ),
                entry(
                    "c_2",
                    RecordedStop::Other,
                    flash,
                    chat_answer(flash, NEEDED),
                ),
                entry(
                    "c_3",
                    RecordedStop::EndTurn,
                    "jeb",
                    chat_answer(flash, NEEDED),
                ),
                entry(
                    "c_4",
                    RecordedStop::EndTurn,
                    "gemma",
                    chat_answer("gemma", NEEDED),
                ),
                entry(
                    "c_5",
                    RecordedStop::EndTurn,
                    flash,
                    chat_answer(flash, "It is needed."),
                ),
            ],
        );
        let judge = ReplayJudge::new(Cassette::parse(&text)?);
        let live = chat::judgment_for(&request("c_1"), &chat_answer(flash, NEEDED));
        assert_eq!(replay(&judge, "c_1"), live, "replay == live");
        assert_eq!(
            replay(&judge, "c_1").map(|j| j.assessment),
            Ok(Assessment::LikelyNeeded)
        );
        for item in ["c_2", "c_3", "c_4"] {
            assert!(
                matches!(replay(&judge, item), Err(JudgeError::NoAnswer(_))),
                "{item}"
            );
        }
        assert_eq!(replay(&judge, "c_5"), Err(JudgeError::Malformed));
        assert_eq!(
            Cassette::parse(&text)?.prompt(),
            None,
            "not a Decisions prompt"
        );
        for other in ["chat-decisions/2", "fm-decisions/1", "jev-decisions/2"] {
            assert!(matches!(
                Cassette::parse(&chat_cassette(other, &[])),
                Err(CassetteError::UnknownPrompt(p)) if p == other
            ));
        }
        Ok(())
    }
}
