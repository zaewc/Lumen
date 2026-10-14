//! Validating a model's answer (roadmap 8.4, docs/ai/model-contract.md).
//!
//! Every check runs in Rust, whatever the provider claims to enforce:
//!
//! 1. a refusal or truncated answer is no answer;
//! 2. an answer from any model but the pinned one is no answer;
//! 3. the body must be exactly `jev.judgment/1`: unknown fields, unknown enum
//!    values (case-sensitive), duplicates and wrong types are rejected;
//! 4. the judgment must match the request ([`Judgment::validate_for`]): same
//!    item, 1–6 reasons, only evidence that was sent, a short rationale.
//!
//! The rationale is display-only text and is never interpreted.

use std::collections::BTreeSet;

use lumen_application::ports::JudgeError;
use lumen_domain::{
    Assessment, EvidenceId, ItemRef, JevRequest, Judgment, JudgmentConfidence, ModelId, ReasonCode,
    SchemaVersion, UntrustedText,
};
use serde::Deserialize;

/// Schema name of judgments.
const JUDGMENT_SCHEMA: &str = "jev.judgment";

/// Why the model stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopReason {
    /// It finished its answer.
    EndTurn,
    /// It ran out of output tokens: the answer is cut off.
    MaxTokens,
    /// It declined to answer.
    Refusal,
    /// Anything else the provider reports.
    Other,
}

/// A model's raw answer, as an adapter received it.
#[derive(Debug, Clone, Copy)]
pub struct RawResponse<'a> {
    /// Why the model stopped.
    pub stop_reason: StopReason,
    /// The model that answered, as the provider reports it.
    pub model: &'a str,
    /// The answer's JSON text.
    pub body: &'a str,
}

/// The judgment exactly as the model must write it.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Wire {
    schema_version: SchemaVersion,
    item_ref: ItemRef,
    assessment: Assessment,
    confidence: JudgmentConfidence,
    reason_codes: Vec<ReasonCode>,
    evidence_refs: Vec<EvidenceId>,
    injection_suspected: bool,
    rationale: String,
}

/// Validates `response` to `request`, answered by `pinned` model.
///
/// # Errors
///
/// A [`JudgeError`] for any answer that is not a valid judgment for exactly
/// this request; the caller then decides without Jev.
pub fn validate(
    request: &JevRequest,
    pinned: &ModelId,
    response: &RawResponse<'_>,
) -> Result<Judgment, JudgeError> {
    match response.stop_reason {
        StopReason::EndTurn => {}
        StopReason::MaxTokens => return Err(JudgeError::NoAnswer("the answer was cut off")),
        StopReason::Refusal => return Err(JudgeError::NoAnswer("the model refused")),
        StopReason::Other => return Err(JudgeError::NoAnswer("unexpected stop reason")),
    }
    if response.model != pinned.as_str() {
        return Err(JudgeError::NoAnswer("an unexpected model answered"));
    }
    let wire: Wire = serde_json::from_str(response.body).map_err(|_| JudgeError::Malformed)?;
    if wire.schema_version.name() != JUDGMENT_SCHEMA || wire.schema_version.major() != 1 {
        return Err(JudgeError::Malformed);
    }
    let reason_codes: BTreeSet<ReasonCode> = wire.reason_codes.iter().copied().collect();
    let evidence_refs: BTreeSet<EvidenceId> = wire.evidence_refs.iter().copied().collect();
    if reason_codes.len() != wire.reason_codes.len()
        || evidence_refs.len() != wire.evidence_refs.len()
    {
        return Err(JudgeError::Malformed);
    }
    let judgment = Judgment {
        item_ref: wire.item_ref,
        assessment: wire.assessment,
        confidence: wire.confidence,
        reason_codes,
        evidence_refs,
        injection_suspected: wire.injection_suspected,
        rationale: UntrustedText::new(wire.rationale.trim()),
    };
    judgment
        .validate_for(&request.item_ref, &request.evidence_ids())
        .map_err(JudgeError::Rejected)?;
    Ok(judgment)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use lumen_domain::{
        ByteCount, CandidateSummary, Category, CoverageSummary, EvidenceKind, JudgmentRejection,
        RequestEvidence,
    };

    use super::*;

    const MODEL: &str = "claude-sonnet-5-5";

    fn id(byte: u8) -> EvidenceId {
        EvidenceId::from_bytes([byte; 32])
    }

    fn request() -> JevRequest {
        JevRequest {
            schema_version: JevRequest::current_schema().unwrap_or_else(|_| unreachable!()),
            item_ref: ItemRef::new("c_1").unwrap_or_else(|_| unreachable!()),
            candidate: CandidateSummary {
                category: Category::AppCache,
                path_display: UntrustedText::new("~/Library/Caches/x"),
                size_bytes: ByteCount::ZERO,
                reclaimable_bytes: ByteCount::ZERO,
            },
            evidence: vec![RequestEvidence {
                id: id(1),
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

    fn body(edit: impl FnOnce(&mut serde_json::Value)) -> String {
        let mut value = serde_json::json!({
            "schema_version": "jev.judgment/1",
            "item_ref": "c_1",
            "assessment": "likely_removable",
            "confidence": "medium",
            "reason_codes": ["owner_app_absent", "regeneratable_cache"],
            "evidence_refs": [id(1).to_string()],
            "injection_suspected": false,
            "rationale": "  Cache of an uninstalled app.  ",
        });
        edit(&mut value);
        value.to_string()
    }

    fn check(stop_reason: StopReason, model: &str, body: &str) -> Result<Judgment, JudgeError> {
        let pinned = ModelId::new(MODEL).unwrap_or_else(|_| unreachable!());
        validate(
            &request(),
            &pinned,
            &RawResponse {
                stop_reason,
                model,
                body,
            },
        )
    }

    #[test]
    fn a_valid_answer_becomes_a_judgment() {
        let judgment = check(StopReason::EndTurn, MODEL, &body(|_| {}));
        let Ok(judgment) = judgment else {
            unreachable!("{judgment:?}");
        };
        assert_eq!(judgment.assessment, Assessment::LikelyRemovable);
        assert_eq!(judgment.rationale.raw(), "Cache of an uninstalled app.");
        assert_eq!(judgment.evidence_refs, BTreeSet::from([id(1)]));
    }

    #[test]
    fn non_answers_are_rejected() {
        let ok = body(|_| {});
        assert!(matches!(
            check(StopReason::Refusal, MODEL, &ok),
            Err(JudgeError::NoAnswer(_))
        ));
        assert!(matches!(
            check(StopReason::MaxTokens, MODEL, &ok),
            Err(JudgeError::NoAnswer(_))
        ));
        assert!(matches!(
            check(StopReason::Other, MODEL, &ok),
            Err(JudgeError::NoAnswer(_))
        ));
        assert!(matches!(
            check(StopReason::EndTurn, "claude-other", &ok),
            Err(JudgeError::NoAnswer(_))
        ));
    }

    #[test]
    fn malformed_bodies_are_rejected() {
        let cases: Vec<String> = vec![
            "not json".to_owned(),
            body(|v| v["action"] = serde_json::json!("delete")),
            body(|v| v["assessment"] = serde_json::json!("Likely_Removable")),
            body(|v| v["assessment"] = serde_json::json!("remove_now")),
            body(|v| v["confidence"] = serde_json::json!(0.9)),
            body(|v| v["schema_version"] = serde_json::json!("jev.judgment/2")),
            body(|v| v["schema_version"] = serde_json::json!("jev.other/1")),
            body(|v| {
                v["reason_codes"] = serde_json::json!(["owner_app_absent", "owner_app_absent"]);
            }),
            body(|v| v["evidence_refs"] = serde_json::json!(["not-an-id"])),
            body(|v| {
                if let Some(o) = v.as_object_mut() {
                    o.remove("rationale");
                }
            }),
        ];
        for case in cases {
            assert_eq!(
                check(StopReason::EndTurn, MODEL, &case),
                Err(JudgeError::Malformed),
                "{case}"
            );
        }
    }

    #[test]
    fn judgments_must_answer_this_request() {
        let rejected =
            |edit: fn(&mut serde_json::Value)| check(StopReason::EndTurn, MODEL, &body(edit));
        assert_eq!(
            rejected(|v| v["item_ref"] = serde_json::json!("c_2")),
            Err(JudgeError::Rejected(JudgmentRejection::ItemMismatch))
        );
        assert_eq!(
            rejected(|v| v["evidence_refs"] =
                serde_json::json!([EvidenceId::from_bytes([9; 32]).to_string()])),
            Err(JudgeError::Rejected(JudgmentRejection::UnknownEvidence))
        );
        assert_eq!(
            rejected(|v| v["evidence_refs"] = serde_json::json!([])),
            Err(JudgeError::Rejected(JudgmentRejection::NoEvidence))
        );
        assert_eq!(
            rejected(|v| v["reason_codes"] = serde_json::json!([])),
            Err(JudgeError::Rejected(JudgmentRejection::ReasonCount))
        );
        assert_eq!(
            rejected(|v| v["rationale"] = serde_json::json!("x".repeat(281))),
            Err(JudgeError::Rejected(JudgmentRejection::RationaleTooLong))
        );
    }
}
