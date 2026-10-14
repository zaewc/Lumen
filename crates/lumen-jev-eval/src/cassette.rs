//! Cassettes (`jev.cassette/1`) and replaying them.

use std::collections::BTreeMap;

use lumen_application::ports::{JudgeError, JudgeFuture, JudgeModel};
use lumen_domain::{JevRequest, JudgeDescriptor, PayloadHash};
use lumen_jev::response::{RawResponse, StopReason, validate};
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
        Ok(cassette)
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

impl JudgeModel for ReplayJudge {
    fn descriptor(&self) -> Option<JudgeDescriptor> {
        Some(self.judge.clone())
    }

    fn judge<'a>(&'a self, request: &'a JevRequest) -> JudgeFuture<'a> {
        let answer = match self.answers.get(&request.hash()) {
            Some(recorded) => validate(
                request,
                &self.judge.model,
                &RawResponse {
                    stop_reason: recorded.stop_reason.into(),
                    model: &recorded.model,
                    body: &recorded.body,
                },
            ),
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
}
