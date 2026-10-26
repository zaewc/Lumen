//! Consulting Jev for one candidate (roadmap 8.5, docs/ai/architecture.md).
//!
//! The policy decides first, without Jev. Jev is asked only when that decision
//! is `Quarantine` and a judge is configured: a judgment can only demote a
//! proposed quarantine to `Review` (ADR-0020), so asking about anything else
//! could never change a decision. That keeps protected and review items'
//! metadata out of prompts and saves the provider's quota. A judgment is then fed
//! back into the policy, which can only become more cautious. Any judge error
//! leaves the first decision in place: the decision never fails because of Jev.
//!
//! Every consultation is traced (`jev.trace/1`), accepted or not.

use lumen_domain::{
    JevRequest, JevTrace, PolicyDecision, SchemaVersion, Timestamp, TraceValidation, Verdict,
};
use lumen_policy::{PolicyError, PolicyInput, evaluate};

use crate::ports::{Clock, JudgeError, JudgeModel};

/// A decision and, if Jev was consulted, its trace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Consulted {
    /// The final decision.
    pub decision: PolicyDecision,
    /// The consultation, if one happened.
    pub trace: Option<JevTrace>,
}

/// Whole milliseconds from `start` to `end`, saturating.
fn millis(start: Timestamp, end: Timestamp) -> u32 {
    let nanos =
        |t: Timestamp| i128::from(t.unix_seconds()) * 1_000_000_000 + i128::from(t.subsec_nanos());
    let elapsed = (nanos(end) - nanos(start)).max(0) / 1_000_000;
    u32::try_from(elapsed).unwrap_or(u32::MAX)
}

/// Decides `input`, consulting `judge` with `request` when the policy allows.
///
/// `input.jev` is ignored; the judgment comes only from `judge`.
///
/// # Errors
///
/// Only the policy's own programming errors ([`PolicyError`]); callers treat
/// them as `Keep`.
pub async fn decide(
    judge: &dyn JudgeModel,
    clock: &dyn Clock,
    mut input: PolicyInput,
    request: &JevRequest,
) -> Result<Consulted, PolicyError> {
    input.jev = None;
    let first = evaluate(&input)?;
    let Some(descriptor) = judge.descriptor() else {
        return Ok(Consulted {
            decision: first,
            trace: None,
        });
    };
    // Only a proposed quarantine can be changed by a judgment.
    if first.verdict() != Verdict::Quarantine {
        return Ok(Consulted {
            decision: first,
            trace: None,
        });
    }
    let requested_at = clock.now();
    let answer = judge.judge(request).await;
    let latency_ms = millis(requested_at, clock.now());
    let (decision, validation, judgment) = match answer {
        Ok(judgment) => {
            input.jev = Some(judgment.clone());
            (evaluate(&input)?, TraceValidation::Accepted, Some(judgment))
        }
        Err(JudgeError::Rejected(reason)) => (first, TraceValidation::Rejected(reason), None),
        Err(_) => (first, TraceValidation::Unavailable, None),
    };
    let output_schema = SchemaVersion::new("jev.judgment", 1, 0)
        .unwrap_or_else(|e| unreachable!("constant schema version is valid: {e}"));
    let trace = JevTrace {
        requested_at,
        judge: descriptor,
        policy_version: decision.policy_version(),
        input_schema: request.schema_version.clone(),
        output_schema,
        evidence_hash: input.evidence_hash,
        request_hash: request.hash(),
        response_hash: None,
        validation,
        judgment,
        calibrated_precision_bp: None,
        policy_effect: decision.jev_effect(),
        latency_ms,
        note: None,
    };
    Ok(Consulted {
        decision,
        trace: Some(trace),
    })
}
