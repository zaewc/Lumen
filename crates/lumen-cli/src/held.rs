//! `lumen list`, `lumen restore` and `lumen finalize`: what quarantine holds,
//! and taking items back or removing them for good (roadmap 10.15b,
//! ADR-0015).
//!
//! Items are named by their operation ID, as `lumen quarantine` printed it.
//! Restoring moves an item back without replacing anything; a taken name is
//! reported and the item stays held. Finalizing is the only permanent
//! removal: it acts on one named operation, only a retained one, and only
//! after finding it in the store.

use std::error::Error;
use std::path::Path;

use lumen_application::finalize::{FinalizeOutcome, FinalizeReason, finalize_item};
use lumen_application::ports::{Clock, NotRestored, PreparedItem};
use lumen_application::restore::{RestoreOutcome, restore_item};
use lumen_domain::{ItemLocation, OperationId, QuarantineEvent, QuarantineState, Timestamp};
use lumen_fs_exec::{Executor, original_path};
use lumen_store_sqlite::{JournalEntry, LedgerJournal, LedgerStore, open_ledger};

use crate::quarantine::store_location;

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
            let days = now.unix_seconds().saturating_sub(h.since.unix_seconds()) / 86_400;
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
        "{} item(s) in quarantine. Restore with `lumen restore --db {} --op <ID>`; remove for good with `lumen finalize --db {} --op <ID>`.",
        held.len(),
        db.display(),
        db.display()
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

fn refuse_root() -> Result<(), Box<dyn Error>> {
    if rustix::process::geteuid().is_root() {
        return Err("refusing to act as root; run as the user who owns the files".into());
    }
    Ok(())
}

/// Moves the item of `operation` back to where it was.
pub(crate) fn restore(
    db: &Path,
    operation: OperationId,
    clock: &dyn Clock,
) -> Result<String, Box<dyn Error>> {
    refuse_root()?;
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
    refuse_root()?;
    let mut store = ledger(db)?;
    let held = find(&store, operation)?;
    let executor = executor()?;
    let mut journal = LedgerJournal::new(&mut store, clock);
    let path = path_text(&held.item);
    Ok(
        match finalize_item(
            &executor,
            &mut journal,
            operation,
            held.state,
            &held.item,
            FinalizeReason::UserRequest,
        )? {
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
            FinalizeOutcome::NotFinalizable(state) => {
                format!("Operation {operation} cannot be finalized (state: {state:?}).\n")
            }
        },
    )
}
