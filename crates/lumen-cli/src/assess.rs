//! `lumen assess`: the latest completed scan's candidates with their
//! verdicts and reasons, as text or JSON (roadmap 10.13b). The assessment
//! itself is `lumen_app::assess`.

use lumen_application::assess::Assessed;
use lumen_domain::{ByteCount, Verdict};
use serde_json::{Value, json};

use crate::report::human;

fn verdict_text(verdict: Verdict) -> &'static str {
    match verdict {
        Verdict::Keep => "KEEP",
        Verdict::Review => "REVIEW",
        Verdict::Quarantine => "QUARANTINE",
    }
}

fn reclaim(assessed: &Assessed) -> ByteCount {
    assessed
        .input
        .observed
        .as_ref()
        .map_or(ByteCount::ZERO, |o| o.reclaim.reclaimable_now)
}

/// The report as text.
pub(crate) fn to_text(assessed: &[Assessed]) -> String {
    let mut lines = Vec::new();
    for item in assessed {
        lines.push(format!(
            "{:<10} {:>9}  {}",
            verdict_text(item.decision.verdict()),
            human(reclaim(item)),
            item.candidate.path.display()
        ));
        lines.extend(
            item.reasons
                .iter()
                .map(|reason| format!("           - {reason}")),
        );
    }
    let proposed: u64 = assessed
        .iter()
        .filter(|a| a.decision.verdict() == Verdict::Quarantine)
        .map(|a| reclaim(a).get())
        .sum();
    lines.push(format!(
        "{} candidates; {} proposed for quarantine (nothing has been moved)",
        assessed.len(),
        human(ByteCount::new(proposed))
    ));
    lines.join("\n") + "\n"
}

/// The report as JSON.
pub(crate) fn to_json(assessed: &[Assessed]) -> Value {
    json!({
        "candidates": assessed.iter().map(|item| json!({
            "path": item.candidate.path.display(),
            "verdict": verdict_text(item.decision.verdict()),
            "reclaimable_bytes": reclaim(item).get(),
            "evidence_hash": item.decision.evidence_hash().to_string(),
            "reasons": item.reasons,
        })).collect::<Vec<_>>(),
    })
}
