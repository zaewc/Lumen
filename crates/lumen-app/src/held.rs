//! What quarantine holds, and taking items back or removing them for good
//! (roadmap 10.15b, 10.20, 11.4; ADR-0015).
//!
//! Items are named by their operation ID. Restoring moves an item back
//! without replacing anything; a taken name is reported and the item stays
//! held. Finalizing is the only permanent removal: it acts on one named
//! operation, only a retained one, only after finding it in the store, and
//! only if no visible process uses it there (ADR-0030). [`finalize_expired`]
//! does the same for every retained item quarantined at least
//! [`RETENTION_DAYS`] ago; never for an item held after a failure, which
//! waits for the user.

use std::path::{Path, PathBuf};

use lumen_application::finalize::{FinalizeOutcome, FinalizeReason, finalize_item};
use lumen_application::ports::{Clock, JournalError, PreparedItem};
use lumen_application::restore::{RestoreOutcome, restore_item};
use lumen_domain::{ItemLocation, OperationId, QuarantineEvent, QuarantineState, Timestamp};
use lumen_fs_exec::{Executor, original_path};
use lumen_store_sqlite::{JournalEntry, LedgerJournal, LedgerStore, StoreError, open_ledger};

use crate::assess::{AssessError, with_probes};
use crate::store::{LocationError, PrivilegeError, refuse_privileged, store_location};

/// How long a retained item is kept before [`finalize_expired`] removes it
/// (ADR-0015's default).
pub const RETENTION_DAYS: i64 = 30;

/// Why a held-item operation did not run.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum HeldError {
    /// Moving files is refused for this process.
    #[error(transparent)]
    Privileged(#[from] PrivilegeError),
    /// The store location is unknown.
    #[error(transparent)]
    Location(#[from] LocationError),
    /// No such operation in the journal.
    #[error("no quarantine operation {0}")]
    UnknownOperation(OperationId),
    /// The ledger failed.
    #[error("the ledger failed: {0}")]
    Store(#[from] StoreError),
    /// An outcome could not be journaled.
    #[error("the journal failed: {0}")]
    Journal(#[from] JournalError),
    /// The in-use probe could not be set up.
    #[error(transparent)]
    Probes(#[from] AssessError),
}

/// One item in quarantine, as the journal knows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeldItem {
    /// The operation that moved it.
    pub operation: OperationId,
    /// Retained, or held after a failure.
    pub state: QuarantineState,
    /// Where it was, if the record says.
    pub original: Option<PathBuf>,
    /// When it was quarantined.
    pub since: Timestamp,
    /// The executor's record of it.
    item: PreparedItem,
}

/// What became of one item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome<T> {
    /// The operation.
    pub operation: OperationId,
    /// Where the item was, if the record says.
    pub original: Option<PathBuf>,
    /// What happened.
    pub outcome: T,
}

fn held(entries: &[JournalEntry], state: QuarantineState) -> Option<HeldItem> {
    let intent = entries
        .iter()
        .find(|e| e.event == QuarantineEvent::Journal)?;
    let item = PreparedItem(serde_json::to_vec(&intent.detail).ok()?);
    Some(HeldItem {
        operation: intent.operation,
        state,
        original: original_path(&item),
        since: intent.at,
        item,
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

fn ledger(db: &Path) -> Result<LedgerStore, StoreError> {
    Ok(LedgerStore::new(open_ledger(
        &db.with_file_name("ledger.db"),
    )?))
}

/// Whole days from `since` to `now`.
pub const fn days_between(since: Timestamp, now: Timestamp) -> i64 {
    now.unix_seconds().saturating_sub(since.unix_seconds()) / 86_400
}

/// Every item held in quarantine next to `db`, oldest first.
///
/// # Errors
///
/// [`HeldError::Store`] if the ledger fails.
pub fn list(db: &Path) -> Result<Vec<HeldItem>, HeldError> {
    let mut held: Vec<HeldItem> = ledger(db)?
        .operations()?
        .into_iter()
        .filter(|(state, _)| in_store(*state))
        .filter_map(|(state, entries)| held(&entries, state))
        .collect();
    held.sort_by_key(|h| (h.since, h.operation));
    Ok(held)
}

fn find(store: &LedgerStore, operation: OperationId) -> Result<HeldItem, HeldError> {
    store
        .operations()?
        .into_iter()
        .filter_map(|(state, entries)| held(&entries, state))
        .find(|h| h.operation == operation)
        .ok_or(HeldError::UnknownOperation(operation))
}

fn executor() -> Result<Executor, HeldError> {
    let (base, names) = store_location()?;
    Ok(Executor::new(base, names))
}

/// Moves the item of `operation` back to where it was, never over anything.
///
/// # Errors
///
/// A [`HeldError`] if the process is privileged, the operation is unknown,
/// or the ledger or journal fails.
#[tracing::instrument(name = "restore", skip_all)]
pub fn restore(
    db: &Path,
    operation: OperationId,
    clock: &dyn Clock,
) -> Result<Outcome<RestoreOutcome>, HeldError> {
    refuse_privileged()?;
    let mut store = ledger(db)?;
    let held = find(&store, operation)?;
    let executor = executor()?;
    let mut journal = LedgerJournal::new(&mut store, clock);
    let outcome = restore_item(&executor, &mut journal, operation, held.state, &held.item)?;
    tracing::info!(outcome = restore_code(outcome), "restore finished");
    Ok(Outcome {
        operation,
        original: held.original,
        outcome,
    })
}

/// Removes the item of `operation` for good, at the user's request.
///
/// # Errors
///
/// A [`HeldError`], as for [`restore`].
#[tracing::instrument(name = "finalize", skip_all)]
pub fn finalize(
    db: &Path,
    operation: OperationId,
    clock: &dyn Clock,
) -> Result<Outcome<FinalizeOutcome>, HeldError> {
    refuse_privileged()?;
    let mut store = ledger(db)?;
    let held = find(&store, operation)?;
    let executor = executor()?;
    let mut journal = LedgerJournal::new(&mut store, clock);
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
    tracing::info!(outcome = finalize_code(outcome), "finalize finished");
    Ok(Outcome {
        operation,
        original: held.original,
        outcome,
    })
}

/// A stable code for a restore outcome, for telemetry.
const fn restore_code(outcome: RestoreOutcome) -> &'static str {
    match outcome {
        RestoreOutcome::Restored { .. } => "restored",
        RestoreOutcome::NotRestored(_) => "not_restored",
        RestoreOutcome::NotRestorable(_) => "not_restorable",
    }
}

/// A stable code for a finalization outcome, for telemetry.
const fn finalize_code(outcome: FinalizeOutcome) -> &'static str {
    match outcome {
        FinalizeOutcome::Finalized => "finalized",
        FinalizeOutcome::FoundAtSource => "found_at_source",
        FinalizeOutcome::NotFound => "not_found",
        FinalizeOutcome::Failed { .. } => "failed",
        FinalizeOutcome::MaybeInUse => "maybe_in_use",
        FinalizeOutcome::NotFinalizable(_) => "not_finalizable",
    }
}

/// The retained operations quarantined at least [`RETENTION_DAYS`] ago,
/// oldest first. Items held after a failure are never expired.
fn expired(operations: Vec<(QuarantineState, Vec<JournalEntry>)>, now: Timestamp) -> Vec<HeldItem> {
    let mut held: Vec<HeldItem> = operations
        .into_iter()
        .filter(|(state, _)| *state == QuarantineState::Retained)
        .filter_map(|(state, entries)| held(&entries, state))
        .filter(|h| days_between(h.since, now) >= RETENTION_DAYS)
        .collect();
    held.sort_by_key(|h| (h.since, h.operation));
    held
}

/// Removes for good every retained item whose retention has elapsed, each
/// probed for use first (ADR-0015, ADR-0030). Empty if none is due.
///
/// # Errors
///
/// A [`HeldError`], as for [`restore`].
#[tracing::instrument(name = "finalize_expired", skip_all)]
pub fn finalize_expired(
    db: &Path,
    clock: &dyn Clock,
) -> Result<Vec<Outcome<FinalizeOutcome>>, HeldError> {
    refuse_privileged()?;
    let mut store = ledger(db)?;
    let due = expired(store.operations()?, clock.now());
    if due.is_empty() {
        return Ok(Vec::new());
    }
    let executor = executor()?;
    let mut journal = LedgerJournal::new(&mut store, clock);
    with_probes(|probes| {
        due.into_iter()
            .map(|held| {
                let outcome = finalize_item(
                    &executor,
                    &mut journal,
                    held.operation,
                    held.state,
                    &held.item,
                    FinalizeReason::RetentionElapsed,
                    probes.in_use,
                )?;
                Ok(Outcome {
                    operation: held.operation,
                    original: held.original,
                    outcome,
                })
            })
            .collect::<Result<Vec<_>, HeldError>>()
    })?
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
