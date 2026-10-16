//! `lumen list`, `lumen restore` and `lumen finalize`: what quarantine holds,
//! and taking items back or removing them for good, as text (roadmap 10.15b,
//! 10.20). The operations themselves are `lumen_app::held`.

use std::error::Error;
use std::path::{Path, PathBuf};

use lumen_app::held::{self, Outcome, RETENTION_DAYS, days_between};
use lumen_application::finalize::FinalizeOutcome;
use lumen_application::ports::{Clock, NotRestored};
use lumen_application::restore::RestoreOutcome;
use lumen_domain::{OperationId, QuarantineState};

fn path_text(path: Option<&PathBuf>) -> String {
    path.map_or_else(|| "(unknown path)".into(), |p| p.display().to_string())
}

/// Every item held in quarantine, oldest first.
pub(crate) fn list(db: &Path, clock: &dyn Clock) -> Result<String, Box<dyn Error>> {
    let now = clock.now();
    let held = held::list(db)?;
    let mut lines: Vec<String> = held
        .iter()
        .map(|h| {
            let days = days_between(h.since, now);
            let state = if h.state == QuarantineState::Retained {
                "retained"
            } else {
                "held after a failure"
            };
            format!(
                "{}  {state:<20} {days:>4} d  {}",
                h.operation,
                path_text(h.original.as_ref())
            )
        })
        .collect();
    lines.push(format!(
        "{} item(s) in quarantine. Restore with `lumen restore --db {db} --op <ID>`; remove for good with `lumen finalize --db {db} --op <ID>`, or all retained for {RETENTION_DAYS} days or more with `lumen finalize --db {db} --expired`.",
        held.len(),
        db = db.display(),
    ));
    Ok(lines.join("\n") + "\n")
}

/// Moves the item of `operation` back to where it was.
pub(crate) fn restore(
    db: &Path,
    operation: OperationId,
    clock: &dyn Clock,
) -> Result<String, Box<dyn Error>> {
    let restored = held::restore(db, operation, clock)?;
    let path = path_text(restored.original.as_ref());
    Ok(match restored.outcome {
        RestoreOutcome::Restored {
            parent_replaced,
            verified,
        } => {
            let mut text = format!("Restored {path}.");
            if parent_replaced {
                text.push_str(" Its folder had been recreated since.");
            }
            if !verified {
                text.push_str(" It is back, but did not match its record exactly; check it.");
            }
            text + "\n"
        }
        RestoreOutcome::NotRestored(reason) => format!(
            "Not restored: {}. It stays in quarantine.\n",
            match reason {
                NotRestored::NameTaken =>
                    format!("something now exists at {path}; move it away and try again"),
                NotRestored::OriginUnreachable =>
                    "its original folder cannot be reached safely".into(),
                NotRestored::ItemChanged => "the quarantined item changed".into(),
                NotRestored::Failed => "the move back failed".into(),
            }
        ),
        RestoreOutcome::NotRestorable(state) => {
            format!("Operation {operation} cannot be restored (state: {state:?}).\n")
        }
    })
}

/// Removes the item of `operation` for good.
pub(crate) fn finalize(
    db: &Path,
    operation: OperationId,
    clock: &dyn Clock,
) -> Result<String, Box<dyn Error>> {
    Ok(finalized_text(&held::finalize(db, operation, clock)?))
}

/// What became of one finalization, as a line for the user.
fn finalized_text(finalized: &Outcome<FinalizeOutcome>) -> String {
    let path = path_text(finalized.original.as_ref());
    match finalized.outcome {
        FinalizeOutcome::Finalized => format!("Removed for good: {path}.\n"),
        FinalizeOutcome::FoundAtSource => {
            format!("{path} is back at its original place already; nothing was removed.\n")
        }
        FinalizeOutcome::NotFound => {
            format!("{path} is not in the quarantine store; nothing was removed.\n")
        }
        FinalizeOutcome::Failed { removed } => {
            format!("Could not remove {path} completely ({removed:?}); run finalize again.\n")
        }
        FinalizeOutcome::MaybeInUse => format!(
            "{path} may be in use, or that cannot be checked; nothing was removed. Close the programs that use it and try again.\n"
        ),
        FinalizeOutcome::NotFinalizable(state) => format!(
            "Operation {} cannot be finalized (state: {state:?}).\n",
            finalized.operation
        ),
    }
}

/// Removes for good every retained item whose retention has elapsed.
pub(crate) fn finalize_expired(db: &Path, clock: &dyn Clock) -> Result<String, Box<dyn Error>> {
    let outcomes = held::finalize_expired(db, clock)?;
    if outcomes.is_empty() {
        return Ok(format!(
            "No item has been in quarantine for {RETENTION_DAYS} days; nothing was removed.\n"
        ));
    }
    let removed = outcomes
        .iter()
        .filter(|o| o.outcome == FinalizeOutcome::Finalized)
        .count();
    let mut text: String = outcomes.iter().map(finalized_text).collect();
    let summary = format!(
        "{removed} of {} item(s) past {RETENTION_DAYS} days removed for good.\n",
        outcomes.len()
    );
    text.push_str(&summary);
    Ok(text)
}
