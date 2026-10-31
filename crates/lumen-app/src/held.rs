//! What quarantine holds, and taking items back or removing them for good
//! (roadmap 10.15b, 10.20, 11.4; ADR-0015).
//!
//! Items are named by their operation ID. Restoring moves an item back
//! without replacing anything; a taken name is reported and the item stays
//! held. Finalizing is the only permanent removal: it acts on one named
//! operation, only a retained one, only after finding it in the store, and
//! only if no visible process uses it there (ADR-0030). [`finalize_expired`]
//! does the same for every retained item the policy proposed that was
//! quarantined at least [`RETENTION_DAYS`] ago; never for an item held after
//! a failure, an item the person chose (ADR-0051), or an item whose chooser
//! was not recorded: those wait for the user.
//!
//! Interrupted operations (UX-3) are settled before items are listed, and by
//! [`reconcile`] when the desktop app starts, so an item a crash or quit left
//! in the store shows up without another cleanup. An operation reconciliation
//! could not settle is listed as needing the user; nothing acts on it here.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use lumen_application::finalize::{FinalizeOutcome, FinalizeReason, finalize_item};
use lumen_application::observe::below;
use lumen_application::ports::{
    Clock, InUse, InUseProbe, JournalError, PreparedItem, QuarantineExecutor,
};
use lumen_application::reconcile::Resolution;
use lumen_application::restore::{RestoreOutcome, restore_item};
use lumen_domain::{
    ByteCount, Chooser, FileIdentity, ItemLocation, OperationId, QuarantineEvent, QuarantineState,
    RawPath, Timestamp,
};
use lumen_fs_exec::{Executor, original_path};
use lumen_store_sqlite::{JournalEntry, LedgerJournal, LedgerStore, StoreError, open_ledger};

use crate::assess::{AssessError, with_probes};
use crate::space_change::{SpaceChange, SpaceWatch};
use crate::store::{LocationError, PrivilegeError, lock_ledger, refuse_privileged, stores};

/// How long a retained item the policy proposed is kept before
/// [`finalize_expired`] removes it (ADR-0015's default). Items the person
/// chose are never removed automatically (ADR-0051).
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
    /// The ledger could not be locked against other Lumen processes.
    #[error("the ledger could not be locked: {0}")]
    Lock(std::io::Error),
}

/// One item in quarantine, as the journal knows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeldItem {
    /// The operation that moved it.
    pub operation: OperationId,
    /// Retained, held after a failure, or (journaled, moved or verified)
    /// interrupted in a way reconciliation could not settle: it needs the
    /// user, and neither restore nor finalize acts on it.
    pub state: QuarantineState,
    /// Where it was, if the record says.
    pub original: Option<PathBuf>,
    /// When it was quarantined.
    pub since: Timestamp,
    /// What removing it for good would free, as the confirmed plan estimated;
    /// `None` for items quarantined before sizes were recorded.
    pub reclaim: Option<ByteCount>,
    /// Who chose it; `None` if that was not recorded. Only an item the
    /// policy chose is removed when its retention ends.
    pub chosen_by: Option<Chooser>,
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
    /// Observed volume change around permanent removal; unknown for restore.
    pub space_change: Option<SpaceChange>,
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
        reclaim: None,
        chosen_by: None,
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

const fn unsettled(state: QuarantineState) -> bool {
    matches!(
        state,
        QuarantineState::Journaled | QuarantineState::Moved | QuarantineState::Verified
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

/// What one reconciliation did, counted by [`Resolution`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Reconciled {
    /// Never moved; failed where they are.
    pub not_moved: usize,
    /// Moved but not verified; held after a failure, to restore.
    pub rolled_back: usize,
    /// Verified; retained.
    pub retained: usize,
    /// Not found where the journal says; left as they are for the user.
    pub needs_attention: usize,
}

/// Settles interrupted operations in `store`; the caller holds the ledger
/// lock. Nothing is moved, restored or removed: items are only located.
fn settle(
    executor: &dyn QuarantineExecutor,
    store: &mut LedgerStore,
    clock: &dyn Clock,
) -> Result<Reconciled, JournalError> {
    let mut journal = LedgerJournal::new(store, clock);
    let mut count = Reconciled::default();
    for (_, resolution) in lumen_application::reconcile::reconcile(executor, &mut journal)? {
        *match resolution {
            Resolution::NotMoved => &mut count.not_moved,
            Resolution::RollBack => &mut count.rolled_back,
            Resolution::Retained => &mut count.retained,
            Resolution::NeedsAttention => &mut count.needs_attention,
        } += 1;
    }
    if count != Reconciled::default() {
        tracing::info!(
            not_moved = count.not_moved,
            rolled_back = count.rolled_back,
            retained = count.retained,
            needs_attention = count.needs_attention,
            "interrupted operations reconciled"
        );
    }
    Ok(count)
}

/// Settles operations a crash or quit interrupted (ADR-0015), as the desktop
/// app does when it starts. `None`: another Lumen holds the ledger and
/// settles them itself ([`lock_ledger`]). Idempotent.
///
/// # Errors
///
/// A [`HeldError`] if the process is privileged, the store location is
/// unknown, or the ledger, its lock or the journal fails; nothing was
/// moved in any case.
#[tracing::instrument(name = "reconcile", skip_all)]
pub fn reconcile(db: &Path, clock: &dyn Clock) -> Result<Option<Reconciled>, HeldError> {
    refuse_privileged()?;
    reconcile_with(db, &executor()?, clock)
}

fn reconcile_with(
    db: &Path,
    executor: &dyn QuarantineExecutor,
    clock: &dyn Clock,
) -> Result<Option<Reconciled>, HeldError> {
    let Some(_lock) = lock_ledger(db, false).map_err(HeldError::Lock)? else {
        return Ok(None);
    };
    Ok(Some(settle(executor, &mut ledger(db)?, clock)?))
}

/// Every item held in quarantine next to `db`, oldest first, after settling
/// interrupted operations; then the items reconciliation could not settle
/// are listed too. While another Lumen holds the ledger (a cleanup under
/// way), or in a privileged process, nothing is settled and unfinished
/// operations are left out: they may be in flight.
///
/// # Errors
///
/// [`HeldError::Store`] if the ledger fails. A reconciliation that fails is
/// logged, not returned: the list is still worth showing.
pub fn list(db: &Path, clock: &dyn Clock) -> Result<Vec<HeldItem>, HeldError> {
    list_with(
        db,
        &|| refuse_privileged().ok().and_then(|()| executor().ok()),
        clock,
    )
}

/// As [`list`], asking for `executor` only if an operation is unfinished.
fn list_with(
    db: &Path,
    executor: &dyn Fn() -> Option<Executor>,
    clock: &dyn Clock,
) -> Result<Vec<HeldItem>, HeldError> {
    let mut ledger = ledger(db)?;
    let settling = if ledger.unfinished()?.is_empty() {
        None
    } else {
        executor().and_then(|executor| {
            lock_ledger(db, false)
                .inspect_err(|error| tracing::warn!(%error, "the ledger could not be locked"))
                .ok()
                .flatten()
                .map(|lock| (executor, lock))
        })
    };
    if let Some((executor, _lock)) = &settling
        && let Err(error) = settle(executor, &mut ledger, clock)
    {
        tracing::warn!(%error, "interrupted operations could not be reconciled");
    }
    let reclaims = ledger.reclaims()?;
    let choosers = ledger.choosers()?;
    let mut held: Vec<HeldItem> = ledger
        .operations()?
        .into_iter()
        .filter(|(state, _)| in_store(*state) || (settling.is_some() && unsettled(*state)))
        .filter_map(|(state, entries)| held(&entries, state))
        .map(|mut item| {
            item.reclaim = reclaims.get(&item.operation).copied();
            item.chosen_by = choosers.get(&item.operation).copied();
            item
        })
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
    stores()?
        .executor()
        .ok_or(HeldError::Location(LocationError::NoHome))
}

/// The in-use probe for held items (ADR-0030, ADR-0046): a partial answer
/// counts only for an item held below the user's home or on a single-user
/// computer; anywhere else it counts as unknown, which keeps the item.
struct HeldProbe<'a> {
    inner: &'a dyn InUseProbe,
    home: Option<RawPath>,
    single_user: bool,
}

impl<'a> HeldProbe<'a> {
    fn new(inner: &'a dyn InUseProbe) -> Result<Self, HeldError> {
        Ok(Self {
            inner,
            home: std::env::home_dir().and_then(|home| crate::raw_path(&home).ok()),
            single_user: stores()?.single_user,
        })
    }
}

impl InUseProbe for HeldProbe<'_> {
    fn probe(&self, path: &RawPath, expected: &FileIdentity) -> InUse {
        match self.inner.probe(path, expected) {
            InUse::Partial { .. }
                if !self.single_user && !self.home.as_ref().is_some_and(|h| below(path, h)) =>
            {
                InUse::Unknown {
                    reason: "held outside home on a shared computer",
                }
            }
            answer => answer,
        }
    }
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
        space_change: None,
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
    let watch = executor
        .held(&held.item, operation)
        .and_then(|(path, identity)| SpaceWatch::parent(&path, &identity.volume));
    let mut journal = LedgerJournal::new(&mut store, clock);
    let outcome = with_probes(|probes| -> Result<_, HeldError> {
        Ok(finalize_item(
            &executor,
            &mut journal,
            operation,
            held.state,
            &held.item,
            FinalizeReason::UserRequest,
            &HeldProbe::new(probes.in_use)?,
        )?)
    })??;
    tracing::info!(outcome = finalize_code(outcome), "finalize finished");
    Ok(Outcome {
        operation,
        original: held.original,
        outcome,
        space_change: watch.and_then(SpaceWatch::finish),
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

/// The retained operations the policy chose that were quarantined at least
/// [`RETENTION_DAYS`] ago, oldest first. Items held after a failure, items
/// the person chose and items without a recorded chooser are never expired.
fn expired(
    operations: Vec<(QuarantineState, Vec<JournalEntry>)>,
    choosers: &BTreeMap<OperationId, Chooser>,
    now: Timestamp,
) -> Vec<HeldItem> {
    let mut held: Vec<HeldItem> = operations
        .into_iter()
        .filter(|(state, _)| *state == QuarantineState::Retained)
        .filter_map(|(state, entries)| held(&entries, state))
        .filter(|h| choosers.get(&h.operation) == Some(&Chooser::Policy))
        .filter(|h| days_between(h.since, now) >= RETENTION_DAYS)
        .collect();
    held.sort_by_key(|h| (h.since, h.operation));
    held
}

/// Removes for good every retained item the policy proposed whose retention
/// has elapsed, each probed for use first (ADR-0015, ADR-0030). Items the
/// person chose stay until the person acts (ADR-0051). Empty if none is due.
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
    let due = expired(store.operations()?, &store.choosers()?, clock.now());
    if due.is_empty() {
        return Ok(Vec::new());
    }
    let executor = executor()?;
    let mut journal = LedgerJournal::new(&mut store, clock);
    with_probes(|probes| {
        let probe = HeldProbe::new(probes.in_use)?;
        due.into_iter()
            .map(|held| {
                let watch = executor
                    .held(&held.item, held.operation)
                    .and_then(|(path, identity)| SpaceWatch::parent(&path, &identity.volume));
                let outcome = finalize_item(
                    &executor,
                    &mut journal,
                    held.operation,
                    held.state,
                    &held.item,
                    FinalizeReason::RetentionElapsed,
                    &probe,
                )?;
                Ok(Outcome {
                    operation: held.operation,
                    original: held.original,
                    outcome,
                    space_change: watch.and_then(SpaceWatch::finish),
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

    /// Answers `Partial` for everything.
    struct PartialProbe;

    impl InUseProbe for PartialProbe {
        fn probe(&self, _path: &RawPath, _expected: &FileIdentity) -> InUse {
            InUse::Partial { unseen: None }
        }
    }

    #[test]
    fn a_partial_answer_counts_for_held_items_only_where_adr_0046_allows() {
        let raw = |p: &str| {
            RawPath::from_unix_bytes(p.as_bytes().to_vec()).unwrap_or_else(|_| unreachable!())
        };
        let identity = FileIdentity {
            volume: lumen_domain::VolumeId::new("dev:1").unwrap_or_else(|_| unreachable!()),
            file: lumen_domain::FileId::new(7),
        };
        let probe = |single_user: bool| HeldProbe {
            inner: &PartialProbe,
            home: Some(raw("/home/t")),
            single_user,
        };
        let partial = |answer: InUse| matches!(answer, InUse::Partial { .. });
        let home_store = raw("/home/t/.local/share/lumen/quarantine/x");
        let other_drive = raw("/mnt/data/$LumenQuarantine.S-1/x");
        assert!(partial(probe(false).probe(&home_store, &identity)));
        assert!(partial(probe(true).probe(&home_store, &identity)));
        assert!(partial(probe(true).probe(&other_drive, &identity)));
        assert!(matches!(
            probe(false).probe(&other_drive, &identity),
            InUse::Unknown { .. }
        ));
        let no_home = HeldProbe {
            home: None,
            ..probe(false)
        };
        assert!(matches!(
            no_home.probe(&home_store, &identity),
            InUse::Unknown { .. }
        ));
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
        let choosers = (1..=6).map(|n| (op(n), Chooser::Policy)).collect();
        let due: Vec<OperationId> = expired(
            vec![
                operation(1, 100, QuarantineState::Retained),
                operation(2, 69, QuarantineState::Retained),
                operation(3, 71, QuarantineState::Retained),
                operation(4, 60, failed),
                operation(5, 60, QuarantineState::Restored),
                operation(6, 60, QuarantineState::Retained),
            ],
            &choosers,
            day(100),
        )
        .iter()
        .map(|h| h.operation)
        .collect();
        assert_eq!(due, [op(6), op(2)], "30 days or more, oldest first");
    }

    #[test]
    fn items_the_person_chose_or_of_unknown_chooser_never_expire() {
        let choosers = BTreeMap::from([(op(1), Chooser::Person), (op(2), Chooser::Policy)]);
        let due: Vec<OperationId> = expired(
            vec![
                operation(1, 0, QuarantineState::Retained),
                operation(2, 0, QuarantineState::Retained),
                operation(3, 0, QuarantineState::Retained),
            ],
            &choosers,
            day(1_000),
        )
        .iter()
        .map(|h| h.operation)
        .collect();
        assert_eq!(due, [op(2)], "only what the policy proposed expires");
    }
}

/// Interrupted cleanups (UX-3), with the real executor in a fixture: the
/// items and the store are the fixture's, never the user's.
#[cfg(all(test, unix))]
mod interrupted {
    use std::error::Error;

    use lumen_testkit::{Fixture, Node};

    use super::*;
    use crate::{SystemClock, raw_path};

    fn op(n: u8) -> OperationId {
        format!("0190f0d2-6c1e-7a3b-8c4d-5e6f708193{n:02x}")
            .parse()
            .unwrap_or_else(|_| unreachable!())
    }

    fn states(items: &[HeldItem]) -> Vec<(OperationId, QuarantineState)> {
        items.iter().map(|h| (h.operation, h.state)).collect()
    }

    #[test]
    fn startup_reconciliation_lists_what_a_crash_left_once_and_only_once()
    -> Result<(), Box<dyn Error>> {
        let names = ["moved", "verified", "unmoved", "changed"];
        let nodes: Vec<Node> = names
            .iter()
            .map(|name| Node::File {
                path: format!("items/{name}"),
                contents: vec![7; 64],
            })
            .collect();
        let fx = Fixture::build(&nodes)?;
        let root = std::fs::canonicalize(fx.root())?;
        let db = root.join("index.db");
        let executor = Executor::new(&root, vec![b"Quarantine".to_vec()]);
        let mut store = ledger(&db)?;
        // Each operation stops where a crash could stop it.
        let crashes: [(u8, &[QuarantineEvent], bool); 4] = [
            (1, &[], true),
            (2, &[QuarantineEvent::Move, QuarantineEvent::Verify], true),
            (3, &[], false),
            (4, &[QuarantineEvent::Move], false),
        ];
        for ((n, after, moved), name) in crashes.into_iter().zip(names) {
            let item = executor
                .prepare(
                    &raw_path(&root)?,
                    &[b"items".to_vec(), name.as_bytes().to_vec()],
                )
                .map_err(|e| format!("{e:?}"))?;
            let detail = serde_json::from_slice(&item.0)?;
            let entry = |event| JournalEntry {
                operation: op(n),
                event,
                at: SystemClock.now(),
                detail: serde_json::json!({}),
            };
            store.journal(&JournalEntry {
                detail,
                ..entry(QuarantineEvent::Journal)
            })?;
            if moved {
                executor
                    .quarantine(&item, op(n), b"{}")
                    .map_err(|e| format!("{e:?}"))?;
            }
            for event in after {
                store.journal(&entry(*event))?;
            }
        }
        // The fourth changed after its intent: it is neither where it was
        // recorded nor in the store.
        std::fs::write(root.join("items/changed"), [8; 65])?;
        assert!(
            list_with(&db, &|| None, &SystemClock)?.is_empty(),
            "unsettled, nothing is listed"
        );

        let settled = reconcile_with(&db, &executor, &SystemClock)?;
        assert_eq!(
            settled,
            Some(Reconciled {
                not_moved: 1,
                rolled_back: 1,
                retained: 1,
                needs_attention: 1,
            })
        );
        let rolled_back = QuarantineState::Failed {
            reason: lumen_domain::QuarantineFailure::VerificationFailed,
            at: ItemLocation::Quarantine,
        };
        let listed = list_with(&db, &|| Some(executor.clone()), &SystemClock)?;
        assert_eq!(
            states(&listed),
            [
                (op(1), rolled_back),
                (op(2), QuarantineState::Retained),
                (op(4), QuarantineState::Moved),
            ],
            "held after a failure, retained, and one that needs the person"
        );
        assert_eq!(listed[0].original, Some(root.join("items/moved")));
        assert!(root.join("items/unmoved").exists(), "never moved");
        assert!(root.join("items/changed").exists(), "left as it is");

        let journal = ledger(&db)?.operations()?;
        let again = reconcile_with(&db, &executor, &SystemClock)?;
        assert_eq!(
            again,
            Some(Reconciled {
                needs_attention: 1,
                ..Reconciled::default()
            })
        );
        assert_eq!(ledger(&db)?.operations()?, journal, "idempotent");

        // While another Lumen holds the ledger, an unfinished operation may
        // be in flight: nothing is settled, and it is not listed.
        let _other = lock_ledger(&db, false)?.ok_or("the lock is free")?;
        assert_eq!(reconcile_with(&db, &executor, &SystemClock)?, None);
        let busy = list_with(&db, &|| Some(executor.clone()), &SystemClock)?;
        assert_eq!(states(&busy), states(&listed[..2]));
        Ok(())
    }
}
