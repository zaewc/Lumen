//! `lumen list`, `lumen restore` and `lumen finalize`: what quarantine holds,
//! and taking items back or removing them for good (roadmap 10.15b,
//! ADR-0015).
//!
//! Items are named by their operation ID, as `lumen quarantine` printed it.
//! Restoring moves an item back without replacing anything; a taken name is
//! reported and the item stays held. Finalizing is the only permanent
//! removal: it acts on one named operation, only a retained one, only after
//! finding it in the store, and only if no visible process uses it there
//! (ADR-0030). `--expired` does the same for every retained item quarantined
//! at least 30 days ago (ADR-0015's retention); never for an item held after
//! a failure, which waits for the user.

use std::error::Error;
use std::path::Path;

use lumen_application::finalize::{FinalizeOutcome, FinalizeReason, finalize_item};
use lumen_application::ports::{Clock, NotRestored, PreparedItem};
use lumen_application::restore::{RestoreOutcome, restore_item};
use lumen_domain::{ItemLocation, OperationId, QuarantineEvent, QuarantineState, Timestamp};
use lumen_fs_exec::{Executor, original_path};
use lumen_store_sqlite::{JournalEntry, LedgerJournal, LedgerStore, open_ledger};

use crate::quarantine::{refuse_privileged, store_location};
use lumen_app::with_probes;

/// One operation as the journal knows it.
struct Held {
    operation: OperationId,
    state: QuarantineState,
    item: PreparedItem,
    since: Timestamp,
}

fn held(entries: &[JournalEntry], state: QuarantineState) -> Option<Held> {
    let intent = entries
        .iter()
        .find(|e| e.event == QuarantineEvent::Journal)?;
    Some(Held {
        operation: intent.operation,
        state,
        item: PreparedItem(serde_json::to_vec(&intent.detail).ok()?),
        since: intent.at,
    })
}

const fn in_store(state: QuarantineState) -> bool {
    matches!(
        state,
        QuarantineState::Retained
            | QuarantineState::Failed {
                at: ItemLocation::Quarantine,
                ..
            }
    )
}

fn ledger(db: &Path) -> Result<LedgerStore, Box<dyn Error>> {
    Ok(LedgerStore::new(open_ledger(
        &db.with_file_name("ledger.db"),
    )?))
}

fn path_text(item: &PreparedItem) -> String {
    original_path(item).map_or_else(|| "(unknown path)".into(), |p| p.display().to_string())
}

/// Every item held in quarantine, oldest first.
pub(crate) fn list(db: &Path, clock: &dyn Clock) -> Result<String, Box<dyn Error>> {
    let now = clock.now();
    let mut held: Vec<Held> = ledger(db)?
        .operations()?
        .into_iter()
        .filter(|(state, _)| in_store(*state))
        .filter_map(|(state, entries)| held(&entries, state))
        .collect();
    held.sort_by_key(|h| (h.since, h.operation));
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
                path_text(&h.item)
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

fn find(store: &LedgerStore, operation: OperationId) -> Result<Held, Box<dyn Error>> {
    store
        .operations()?
        .into_iter()
        .filter_map(|(state, entries)| held(&entries, state))
        .find(|h| h.operation == operation)
        .ok_or_else(|| format!("no quarantine operation {operation}").into())
}

fn executor() -> Result<Executor, Box<dyn Error>> {
    let (base, names) = store_location()?;
    Ok(Executor::new(base, names))
}

/// Moves the item of `operation` back to where it was.
pub(crate) fn restore(
    db: &Path,
    operation: OperationId,
    clock: &dyn Clock,
) -> Result<String, Box<dyn Error>> {
    refuse_privileged()?;
    let mut store = ledger(db)?;
    let held = find(&store, operation)?;
    let executor = executor()?;
    let mut journal = LedgerJournal::new(&mut store, clock);
    let path = path_text(&held.item);
    Ok(
        match restore_item(&executor, &mut journal, operation, held.state, &held.item)? {
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
        },
    )
}

/// Removes the item of `operation` for good.
pub(crate) fn finalize(
    db: &Path,
    operation: OperationId,
    clock: &dyn Clock,
) -> Result<String, Box<dyn Error>> {
    refuse_privileged()?;
    let mut store = ledger(db)?;
    let held = find(&store, operation)?;
    let executor = executor()?;
    let mut journal = LedgerJournal::new(&mut store, clock);
    let path = path_text(&held.item);
    let outcome = with_probes(|probes| {
        finalize_item(
            &executor,
            &mut journal,
            operation,
            held.state,
            &held.item,
            FinalizeReason::UserRequest,
            probes.in_use,
        )
    })??;
    Ok(finalized_text(outcome, &path, operation))
}

/// What became of one finalization, as a line for the user.
fn finalized_text(outcome: FinalizeOutcome, path: &str, operation: OperationId) -> String {
    match outcome {
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
        FinalizeOutcome::NotFinalizable(state) => {
            format!("Operation {operation} cannot be finalized (state: {state:?}).\n")
        }
    }
}

/// How long a retained item is kept before `--expired` removes it
/// (ADR-0015's default).
const RETENTION_DAYS: i64 = 30;

/// Whole days from `since` to `now`.
const fn days_between(since: Timestamp, now: Timestamp) -> i64 {
    now.unix_seconds().saturating_sub(since.unix_seconds()) / 86_400
}

/// The retained operations quarantined at least [`RETENTION_DAYS`] ago,
/// oldest first. Items held after a failure are never expired.
fn expired(operations: Vec<(QuarantineState, Vec<JournalEntry>)>, now: Timestamp) -> Vec<Held> {
    let mut held: Vec<Held> = operations
        .into_iter()
        .filter(|(state, _)| *state == QuarantineState::Retained)
        .filter_map(|(state, entries)| held(&entries, state))
        .filter(|h| days_between(h.since, now) >= RETENTION_DAYS)
        .collect();
    held.sort_by_key(|h| (h.since, h.operation));
    held
}

/// Removes for good every retained item whose retention has elapsed, each
/// probed for use first (ADR-0015, ADR-0030).
pub(crate) fn finalize_expired(db: &Path, clock: &dyn Clock) -> Result<String, Box<dyn Error>> {
    refuse_privileged()?;
    let mut store = ledger(db)?;
    let due = expired(store.operations()?, clock.now());
    if due.is_empty() {
        return Ok(format!(
            "No item has been in quarantine for {RETENTION_DAYS} days; nothing was removed.\n"
        ));
    }
    let executor = executor()?;
    let mut journal = LedgerJournal::new(&mut store, clock);
    let mut text = String::new();
    let mut removed = 0_usize;
    with_probes(|probes| -> Result<(), Box<dyn Error>> {
        for held in &due {
            let outcome = finalize_item(
                &executor,
                &mut journal,
                held.operation,
                held.state,
                &held.item,
                FinalizeReason::RetentionElapsed,
                probes.in_use,
            )?;
            if outcome == FinalizeOutcome::Finalized {
                removed = removed.saturating_add(1);
            }
            text.push_str(&finalized_text(
                outcome,
                &path_text(&held.item),
                held.operation,
            ));
        }
        Ok(())
    })??;
    let summary = format!(
        "{removed} of {} item(s) past {RETENTION_DAYS} days removed for good.\n",
        due.len()
    );
    text.push_str(&summary);
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn op(n: u8) -> OperationId {
        format!("0190f0d2-6c1e-7a3b-8c4d-5e6f708192{n:02x}")
            .parse()
            .unwrap_or_else(|_| unreachable!())
    }

    fn day(n: i64) -> Timestamp {
        Timestamp::from_unix(n * 86_400, 0).unwrap_or_else(|_| unreachable!())
    }

    /// One operation journaled on `quarantined` and replayed to `state`.
    fn operation(
        n: u8,
        quarantined: i64,
        state: QuarantineState,
    ) -> (QuarantineState, Vec<JournalEntry>) {
        let entry = JournalEntry {
            operation: op(n),
            event: QuarantineEvent::Journal,
            at: day(quarantined),
            detail: serde_json::json!({}),
        };
        (state, vec![entry])
    }

    #[test]
    fn only_retained_items_past_retention_expire() {
        let failed = QuarantineState::Failed {
            at: ItemLocation::Quarantine,
            reason: lumen_domain::QuarantineFailure::IdentityMismatch,
        };
        let due: Vec<OperationId> = expired(
            vec![
                operation(1, 100, QuarantineState::Retained),
                operation(2, 69, QuarantineState::Retained),
                operation(3, 71, QuarantineState::Retained),
                operation(4, 60, failed),
                operation(5, 60, QuarantineState::Restored),
                operation(6, 60, QuarantineState::Retained),
            ],
            day(100),
        )
        .iter()
        .map(|h| h.operation)
        .collect();
        assert_eq!(due, [op(6), op(2)], "30 days or more, oldest first");
    }
}
