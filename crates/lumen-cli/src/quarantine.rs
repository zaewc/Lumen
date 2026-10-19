//! `lumen quarantine`: executes exactly one confirmed plan and reports each
//! item (roadmap 10.14b, ADR-0034). The execution itself is
//! `lumen_app::quarantine`.

use std::error::Error;
use std::path::Path;

use lumen_app::quarantine::{ItemResult, quarantine as execute};
use lumen_application::confirm::Skip;
use lumen_application::ports::{Clock, IdGenerator};
use lumen_domain::{ConfirmationSurface, PlanId, Verdict};

use crate::report::human;

fn skip_text(reason: Skip) -> String {
    match reason {
        Skip::Changed => "it changed or cannot be observed".into(),
        Skip::OutsideRoots => "it is outside the scanned roots".into(),
        Skip::NoLongerProposed(verdict) => format!(
            "the policy now says {}",
            match verdict {
                Verdict::Keep => "KEEP",
                Verdict::Review => "REVIEW",
                Verdict::Quarantine => "QUARANTINE",
            }
        ),
        Skip::ReasonsChanged => "it would now be proposed for different reasons".into(),
        Skip::NotPrepared => "it could not be recorded for a safe move".into(),
        Skip::Manifest => "its manifest could not be written".into(),
    }
}

/// Executes plan `plan` from `db`, confirmed by `confirm` (its full hash).
pub(crate) fn quarantine(
    db: &Path,
    plan: PlanId,
    confirm: &str,
    clock: &dyn Clock,
    ids: &dyn IdGenerator,
) -> Result<String, Box<dyn Error>> {
    let report = execute(
        db,
        plan,
        confirm,
        ConfirmationSurface::CommandLine,
        clock,
        ids,
    )?;
    for warning in &report.warnings {
        eprintln!("warning: {warning}");
    }
    let mut lines = Vec::new();
    if report.resolved > 0 {
        lines.push(format!(
            "Resolved {} interrupted operation(s) from earlier runs.",
            report.resolved
        ));
    }
    for item in &report.items {
        let path = item.path.display();
        lines.push(match item.result {
            ItemResult::Skipped(reason) => {
                format!("REVIEW      {path}  (skipped: {})", skip_text(reason))
            }
            ItemResult::Quarantined(operation) => {
                format!("QUARANTINED {path}  (operation {operation})")
            }
            ItemResult::Failed { reason, left } => {
                format!("FAILED      {path}  ({reason:?}; left in {left:?})")
            }
        });
    }
    lines.push(format!(
        "{} now in quarantine (restorable; space is freed only when finalized).",
        human(report.moved)
    ));
    Ok(lines.join("\n") + "\n")
}
