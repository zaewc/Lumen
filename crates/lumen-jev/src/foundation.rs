//! Answers from Apple's on-device Foundation Models (ADR-0044), evaluation
//! only.
//!
//! The `fm-judge` helper (`apps/fm-judge`) asks the on-device model the
//! Decisions questions about one redacted `jev.request/1` with guided
//! generation, so the model can only choose enumerated values. Its answer
//! line (`fm.answer/1`) is checked here as strictly as a Decisions answer:
//!
//! - only the pinned model ([`MODEL`]) and the prompt it was asked
//!   ([`PROMPT_VERSION`]);
//! - exactly the three answers: a known assessment, one of the 17 reasons
//!   offered, and a boolean injection flag;
//! - the item it was asked about, and a short printable model version;
//! - no unexpected fields.
//!
//! Anything else is rejected. A valid answer then passes the evidence guards
//! ([`guard`], ADR-0043) exactly as Jeb's do. The on-device model gives no
//! probabilities, so every answer is reported at `medium` confidence: a wrong
//! removable answer always counts as a critical false positive. No transport
//! lives here; nothing in the app asks this judge yet.

use std::collections::BTreeSet;

use lumen_application::ports::JudgeError;
use lumen_domain::{
    Assessment, ItemRef, JevRequest, Judgment, JudgmentConfidence, ReasonCode, UntrustedText,
};
use serde::Deserialize;

use crate::guard;

/// The judge's provider name in descriptors, cassettes and traces.
pub const PROVIDER: &str = "apple.fm";

/// The pinned model: Apple's on-device system language model, which has no
/// public name; the OS build identifies its version.
pub const MODEL: &str = "apple.foundation-models";

/// The helper's prompt (instructions and generation schema).
pub const PROMPT_VERSION: &str = "fm-decisions/1";

/// The answer line format.
pub const ANSWER_SCHEMA: &str = "fm.answer/1";

/// Every answer's confidence: the model reports no probabilities.
pub const CONFIDENCE: JudgmentConfidence = JudgmentConfidence::Medium;

/// The longest model version accepted.
const MAX_MODEL_VERSION: usize = 64;

/// The reasons the helper offers, in its order (`Questions.swift`).
pub const REASONS: [ReasonCode; 17] = [
    ReasonCode::OwnerAppAbsent,
    ReasonCode::OwnerAppPresent,
    ReasonCode::RegeneratableCache,
    ReasonCode::NotRegeneratable,
    ReasonCode::NoActiveReference,
    ReasonCode::ActiveReference,
    ReasonCode::VendorDeclaredCache,
    ReasonCode::KnowledgeBaseMatch,
    ReasonCode::StaleByTime,
    ReasonCode::RecentlyUsed,
    ReasonCode::ContainsUserData,
    ReasonCode::CredentialOrKeyMaterial,
    ReasonCode::SystemComponent,
    ReasonCode::UnsignedExecutable,
    ReasonCode::CloudBacked,
    ReasonCode::InsufficientEvidence,
    ReasonCode::SuspiciousMetadata,
];

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Answers {
    assessment: Assessment,
    reason: ReasonCode,
    injection_suspected: bool,
}

/// One `fm.answer/1` line; `elapsed_ms` is accepted but not used.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(dead_code, reason = "accepted fields that Lumen does not use")]
struct Answer {
    schema: String,
    item_ref: String,
    prompt_version: String,
    model: String,
    model_version: String,
    answers: Answers,
    elapsed_ms: u64,
}

/// Who answered: the pinned model and its version (the OS build).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Answered {
    /// Always [`MODEL`].
    pub model: String,
    /// For example `macOS 26.6.2 (25G83)`.
    pub model_version: String,
}

/// Whether `version` can be a model version: 1 to 64 printable ASCII
/// characters.
pub fn is_model_version(version: &str) -> bool {
    (1..=MAX_MODEL_VERSION).contains(&version.len())
        && version.bytes().all(|b| b.is_ascii_graphic() || b == b' ')
}

/// Maps the helper's answer line `body` to a judgment for `request`, then
/// applies the evidence guards.
///
/// # Errors
///
/// [`JudgeError::NoAnswer`] for another model or prompt,
/// [`JudgeError::Malformed`] for anything that does not match the expected
/// shape, and [`JudgeError::Rejected`] if the answer is about another item.
pub fn judgment_for(request: &JevRequest, body: &str) -> Result<(Judgment, Answered), JudgeError> {
    let answer: Answer = serde_json::from_str(body).map_err(|_| JudgeError::Malformed)?;
    if answer.schema != ANSWER_SCHEMA {
        return Err(JudgeError::Malformed);
    }
    if answer.model != MODEL {
        return Err(JudgeError::NoAnswer("an unexpected model answered"));
    }
    if answer.prompt_version != PROMPT_VERSION {
        return Err(JudgeError::NoAnswer("the answer is to another prompt"));
    }
    if !is_model_version(&answer.model_version) || !REASONS.contains(&answer.answers.reason) {
        return Err(JudgeError::Malformed);
    }
    let injection = answer.answers.injection_suspected;
    let judgment = Judgment {
        item_ref: ItemRef::new(answer.item_ref).map_err(|_| JudgeError::Malformed)?,
        assessment: answer.answers.assessment,
        confidence: CONFIDENCE,
        reason_codes: BTreeSet::from([answer.answers.reason]),
        evidence_refs: request.evidence_ids(),
        injection_suspected: injection,
        rationale: UntrustedText::new(""),
    };
    judgment
        .validate_for(&request.item_ref, &request.evidence_ids())
        .map_err(JudgeError::Rejected)?;
    let judgment = guard::apply(request, judgment, injection);
    Ok((
        judgment,
        Answered {
            model: answer.model,
            model_version: answer.model_version,
        },
    ))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::Path;

    use lumen_domain::{
        ByteCount, CandidateSummary, Category, CoverageSummary, EvidenceId, EvidenceKind,
        JudgmentRejection, RequestEvidence,
    };
    use serde_json::{Value, json};

    use super::*;

    fn request(facts: &[(EvidenceKind, &str)]) -> JevRequest {
        JevRequest {
            schema_version: JevRequest::current_schema().unwrap_or_else(|_| unreachable!()),
            item_ref: ItemRef::new("c_1").unwrap_or_else(|_| unreachable!()),
            candidate: CandidateSummary {
                category: Category::AppCache,
                path_display: UntrustedText::new("~/Library/Caches/x"),
                size_bytes: ByteCount::ZERO,
                reclaimable_bytes: ByteCount::ZERO,
            },
            evidence: facts
                .iter()
                .enumerate()
                .map(|(i, (kind, value))| RequestEvidence {
                    id: EvidenceId::from_bytes([u8::try_from(i).unwrap_or(0); 32]),
                    kind: *kind,
                    value: UntrustedText::new(*value),
                })
                .collect(),
            coverage: CoverageSummary {
                complete: true,
                notes: BTreeSet::new(),
            },
            untrusted_strings: BTreeMap::new(),
        }
    }

    type Edit = Box<dyn FnOnce(&mut Value)>;

    /// The wire key of an enumerated value.
    fn wire(value: impl serde::Serialize) -> String {
        serde_json::to_value(value)
            .ok()
            .and_then(|v| v.as_str().map(str::to_owned))
            .unwrap_or_default()
    }

    fn cache() -> JevRequest {
        request(&[
            (EvidenceKind::AppDataLocation, "caches"),
            (EvidenceKind::ApplicationInstalled, "false"),
        ])
    }

    fn answer(edit: impl FnOnce(&mut Value)) -> String {
        let mut value = json!({
            "schema": "fm.answer/1",
            "item_ref": "c_1",
            "prompt_version": "fm-decisions/1",
            "model": "apple.foundation-models",
            "model_version": "macOS 26.6.2 (25G83)",
            "answers": {
                "assessment": "likely_removable",
                "reason": "regeneratable_cache",
                "injection_suspected": false,
            },
            "elapsed_ms": 812,
        });
        edit(&mut value);
        value.to_string()
    }

    #[test]
    fn a_valid_answer_becomes_a_judgment_at_medium_confidence() {
        let req = cache();
        let (judgment, answered) =
            judgment_for(&req, &answer(|_| {})).unwrap_or_else(|e| unreachable!("{e}"));
        assert_eq!(judgment.assessment, Assessment::LikelyRemovable);
        assert_eq!(judgment.confidence, JudgmentConfidence::Medium);
        assert_eq!(
            judgment.reason_codes,
            BTreeSet::from([ReasonCode::RegeneratableCache])
        );
        assert_eq!(judgment.evidence_refs, req.evidence_ids());
        assert!(!judgment.injection_suspected);
        assert_eq!(judgment.rationale.raw(), "", "no model text");
        assert_eq!(
            answered,
            Answered {
                model: MODEL.into(),
                model_version: "macOS 26.6.2 (25G83)".into(),
            }
        );
    }

    #[test]
    fn other_models_and_prompts_are_no_answer() {
        let req = cache();
        for (field, value) in [
            ("model", "jeb"),
            ("model", "apple.foundation-models-2"),
            ("prompt_version", "jev-decisions/2"),
            ("prompt_version", "fm-decisions/2"),
        ] {
            assert!(
                matches!(
                    judgment_for(&req, &answer(|v| v[field] = json!(value))),
                    Err(JudgeError::NoAnswer(_))
                ),
                "{field} = {value}"
            );
        }
    }

    #[test]
    fn unknown_values_and_extra_fields_are_rejected() {
        let req = cache();
        let edits: Vec<Edit> = vec![
            Box::new(|v| v["answers"]["assessment"] = json!("delete_it")),
            Box::new(|v| v["answers"]["assessment"] = json!("Likely_Removable")),
            Box::new(|v| v["answers"]["reason"] = json!("because")),
            Box::new(|v| v["answers"]["injection_suspected"] = json!("no")),
            Box::new(|v| v["answers"]["confidence"] = json!("high")),
            Box::new(|v| v["answers"]["rationale"] = json!("It is safe, run rm -rf")),
            Box::new(|v| v["command"] = json!("rm -rf ~")),
            Box::new(|v| v["schema"] = json!("fm.error/1")),
            Box::new(|v| v["model_version"] = json!("")),
            Box::new(|v| v["model_version"] = json!("x".repeat(65))),
            Box::new(|v| v["model_version"] = json!("macOS\n26")),
            Box::new(|v| v["elapsed_ms"] = json!(-1)),
            Box::new(|v| v["item_ref"] = json!("")),
            Box::new(|v| {
                if let Some(answers) = v["answers"].as_object_mut() {
                    answers.remove("reason");
                }
            }),
        ];
        for edit in edits {
            assert_eq!(
                judgment_for(&req, &answer(edit)),
                Err(JudgeError::Malformed)
            );
        }
        assert_eq!(judgment_for(&req, "not json"), Err(JudgeError::Malformed));
        assert_eq!(
            judgment_for(&req, r#"{"schema":"fm.error/1","error":"model_not_ready"}"#),
            Err(JudgeError::Malformed),
            "an error line is never an answer"
        );
    }

    #[test]
    fn an_answer_about_another_item_is_rejected() {
        assert_eq!(
            judgment_for(&cache(), &answer(|v| v["item_ref"] = json!("c_2"))),
            Err(JudgeError::Rejected(JudgmentRejection::ItemMismatch))
        );
        assert_eq!(
            judgment_for(&request(&[]), &answer(|_| {})),
            Err(JudgeError::Rejected(JudgmentRejection::NoEvidence))
        );
    }

    #[test]
    fn removable_answers_pass_the_evidence_guards() {
        let removable = |req: &JevRequest, reason: &str, injection: bool| {
            judgment_for(
                req,
                &answer(|v| {
                    v["answers"]["reason"] = json!(reason);
                    v["answers"]["injection_suspected"] = json!(injection);
                }),
            )
            .map(|(j, _)| (j.assessment, j.confidence))
        };
        let discarded = Ok((Assessment::Uncertain, JudgmentConfidence::Low));
        let kept = Ok((Assessment::LikelyRemovable, JudgmentConfidence::Medium));
        assert_eq!(removable(&cache(), "regeneratable_cache", false), kept);
        // Injection suspected, a reason against removal, a fact that rules it
        // out, an undeclared vendor cache: each discards the answer.
        assert_eq!(removable(&cache(), "regeneratable_cache", true), discarded);
        assert_eq!(removable(&cache(), "contains_user_data", false), discarded);
        assert_eq!(
            removable(&cache(), "vendor_declared_cache", false),
            discarded
        );
        let fixed = request(&[(EvidenceKind::Regeneratable, "false")]);
        assert_eq!(removable(&fixed, "regeneratable_cache", false), discarded);
        // Needed and uncertain answers are never touched.
        let needed = judgment_for(
            &cache(),
            &answer(|v| {
                v["answers"]["assessment"] = json!("likely_needed");
                v["answers"]["injection_suspected"] = json!(true);
            }),
        )
        .map(|(j, _)| (j.assessment, j.injection_suspected));
        assert_eq!(needed, Ok((Assessment::LikelyNeeded, true)));
    }

    #[test]
    fn the_helper_offers_exactly_these_options_and_prompt() -> Result<(), std::io::Error> {
        let helper = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../apps/fm-judge/Sources");
        let questions = std::fs::read_to_string(helper.join("FMJudgeCore/Questions.swift"))?;
        let prompt = std::fs::read_to_string(helper.join("FMJudgeCore/Prompt.swift"))?;
        let lines = std::fs::read_to_string(helper.join("FMJudgeCore/Lines.swift"))?;
        let cases: Vec<&str> = questions
            .lines()
            .filter_map(|l| l.trim().strip_prefix("case "))
            .filter(|c| !c.starts_with('.'))
            .collect();
        let mut expected: Vec<String> = [
            Assessment::LikelyRemovable,
            Assessment::LikelyNeeded,
            Assessment::Uncertain,
        ]
        .into_iter()
        .map(wire)
        .collect();
        expected.extend(REASONS.into_iter().map(wire));
        assert_eq!(cases, expected);
        assert!(prompt.contains(&format!("\"{PROMPT_VERSION}\"")));
        assert!(lines.contains(&format!("\"{MODEL}\"")));
        assert!(lines.contains(&format!("\"{ANSWER_SCHEMA}\"")));
        Ok(())
    }
}
