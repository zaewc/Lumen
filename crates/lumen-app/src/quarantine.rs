//! Executing exactly one confirmed plan (roadmap 10.14b, 11.4; ADR-0034,
//! ADR-0015).
//!
//! Refuses unless the process is not privileged (ADR-0005), the plan exists,
//! the confirmation is its full hash, and it is at most 24 hours old and
//! still proposed. Then, in order: the ledger is locked against other
//! Lumen processes until the end ([`lock_ledger`]); interrupted operations
//! from earlier runs are reconciled; the plan is confirmed and started; every item is decided
//! again on fresh evidence and either skipped or quarantined through the
//! journaled, no-replace path; the plan finishes. Everything moved stays
//! restorable in the store.

use std::path::Path;

use lumen_application::confirm::{Confirmed, Confirming, Skip, confirm_item};
use lumen_application::ports::{Clock, IdGenerator, JournalError, NoopJudge};
use lumen_application::quarantine::{Outcome, quarantine_item};
use lumen_application::reconcile::reconcile;
use lumen_domain::{
    ByteCount, Chooser, Confirmation, ConfirmationSurface, DeviceId, ItemLocation, OperationId,
    PlanEvent, PlanId, QuarantineFailure, RawPath, Timestamp,
};
use lumen_store_sqlite::{
    IndexStore, LedgerJournal, LedgerStore, StoreError, StoredPlan, open_index, open_ledger,
};

use crate::assess::{AssessError, block_on, prepare};
use crate::probes::with_decision_probes;
use crate::store::{LocationError, PrivilegeError, lock_ledger, refuse_privileged, stores};

/// How long a plan may wait for its confirmation (ADR-0034).
pub const MAX_PLAN_AGE_SECS: i64 = 24 * 60 * 60;

/// Why a plan was not executed. Nothing was moved in any of these cases.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum QuarantineError {
    /// Moving files is refused for this process.
    #[error(transparent)]
    Privileged(#[from] PrivilegeError),
    /// The store location is unknown.
    #[error(transparent)]
    Location(#[from] LocationError),
    /// No such plan.
    #[error("plan {0} does not exist; make a plan first")]
    UnknownPlan(PlanId),
    /// The confirmation is not the plan's full hash.
    #[error("the confirmation does not match plan {0}; nothing was moved")]
    Mismatch(PlanId),
    /// The plan waited too long.
    #[error("plan {0} is older than 24 hours; make a new plan")]
    TooOld(PlanId),
    /// The plan was confirmed or executed before.
    #[error("plan {0} was already confirmed or executed; make a new plan")]
    AlreadyConfirmed(PlanId),
    /// The plan uninstalls an application (ADR-0062); it is executed only as
    /// an uninstall.
    #[error("plan {0} uninstalls an application, it is not a cleanup plan; nothing was moved")]
    NotACleanup(PlanId),
    /// The index database or the ledger failed.
    #[error("the database failed: {0}")]
    Store(#[from] StoreError),
    /// An outcome could not be journaled.
    #[error("the journal failed: {0}")]
    Journal(#[from] JournalError),
    /// The fresh assessment could not be prepared.
    #[error(transparent)]
    Assess(#[from] AssessError),
    /// This installation's device ID could not be read or written.
    #[error("the device ID could not be read or written: {0}")]
    Device(String),
    /// The quarantine store could not be created or opened.
    #[error("the quarantine store could not be prepared: {0}")]
    StoreUnavailable(String),
    /// The ledger could not be locked against other Lumen processes.
    #[error("the ledger could not be locked: {0}")]
    Lock(std::io::Error),
}

/// What became of one planned item.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemResult {
    /// Decided again and not moved; it is `REVIEW` at most now.
    Skipped(Skip),
    /// Moved into the store as `operation`.
    Quarantined(OperationId),
    /// The move failed; `left` says where the item is.
    Failed {
        /// Why.
        reason: QuarantineFailure,
        /// Where the item is.
        left: ItemLocation,
    },
}

/// One planned item and what became of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemReport {
    /// The item's path, as planned.
    pub path: RawPath,
    /// What it would free, as planned.
    pub reclaim: ByteCount,
    /// Who chose it (ADR-0051).
    pub chosen_by: Chooser,
    /// What happened.
    pub result: ItemResult,
}

/// What an execution did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuarantineReport {
    /// The plan.
    pub plan: PlanId,
    /// Interrupted operations from earlier runs that were resolved first.
    pub resolved: usize,
    /// Each item, in plan order.
    pub items: Vec<ItemReport>,
    /// What the quarantined items will free once finalized.
    pub moved: ByteCount,
    /// Problems that did not stop the execution, such as a store that could
    /// not be excluded from backups.
    pub warnings: Vec<String>,
}

/// This installation's device ID, kept next to the database.
pub(crate) fn device(db: &Path, ids: &dyn IdGenerator) -> Result<DeviceId, QuarantineError> {
    let path = db.with_file_name("device-id");
    match std::fs::read_to_string(&path) {
        Ok(text) => {
            return text.trim().parse().map_err(|_| {
                QuarantineError::Device(format!("{} is not a device ID", path.display()))
            });
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(QuarantineError::Device(format!(
                "{}: {error}",
                path.display()
            )));
        }
    }
    // A fresh time-ordered ID; the operation ID generator gives one.
    let id: DeviceId = ids
        .operation_id()
        .to_string()
        .parse()
        .map_err(|_| QuarantineError::Device("a fresh ID was rejected".into()))?;
    std::fs::write(&path, format!("{id}\n"))
        .map_err(|e| QuarantineError::Device(format!("{}: {e}", path.display())))?;
    Ok(id)
}

/// Creates the store if needed and asks the system not to back it up
/// (macOS, roadmap 9.2b); a failed exclusion is returned as a warning. On
/// Windows the executor opens the store when it first moves an item, created
/// private and not content-indexed (ADR-0033).
#[cfg(unix)]
pub(crate) fn prepare_store(
    base: &Path,
    names: &[Vec<u8>],
) -> Result<Vec<String>, QuarantineError> {
    use std::os::unix::fs::MetadataExt as _;

    use lumen_fs_exec::open_store;
    let unavailable = |e: &dyn std::fmt::Display| QuarantineError::StoreUnavailable(e.to_string());
    let volume = std::fs::metadata(base).map_err(|e| unavailable(&e))?.dev();
    let names: Vec<&[u8]> = names.iter().map(Vec::as_slice).collect();
    let store = open_store(base, &names, volume).map_err(|e| unavailable(&e))?;
    #[cfg(target_os = "macos")]
    let warnings: Vec<String> =
        lumen_platform_macos::exclude_from_backup(std::os::fd::AsFd::as_fd(store.handle()))
            .err()
            .map(|error| {
                format!("the quarantine store could not be excluded from Time Machine: {error}")
            })
            .into_iter()
            .collect();
    #[cfg(not(target_os = "macos"))]
    let warnings = Vec::new();
    drop(store);
    Ok(warnings)
}

/// Records what each retained item will free, for the quarantine's totals,
/// and who chose it, for retention (ADR-0051). The items are already safe in
/// the store, so a failure here only warns: an item without a recorded
/// chooser is never removed automatically.
fn record_items(ledger: &mut LedgerStore, items: &[ItemReport], warnings: &mut Vec<String>) {
    for item in items {
        let ItemResult::Quarantined(operation) = item.result else {
            continue;
        };
        if let Err(error) = ledger.record_reclaim(operation, item.reclaim) {
            warnings.push(format!(
                "the size of a quarantined item was not recorded: {error}"
            ));
        }
        if let Err(error) = ledger.record_chooser(operation, item.chosen_by) {
            warnings.push(format!(
                "who chose a quarantined item was not recorded, so it waits for you to remove it: {error}"
            ));
        }
    }
}

fn age_secs(now: Timestamp, then: Timestamp) -> i64 {
    now.unix_seconds().saturating_sub(then.unix_seconds())
}

/// The stored plan, if it may be executed now with `confirm` (ADR-0034): the
/// full hash matches, and it is at most 24 hours old. The caller refuses a
/// privileged process first.
pub(crate) fn checked_plan(
    index: &IndexStore,
    plan: PlanId,
    confirm: &str,
    now: Timestamp,
) -> Result<StoredPlan, QuarantineError> {
    let stored = index
        .plan(plan)?
        .ok_or(QuarantineError::UnknownPlan(plan))?;
    if stored.plan.hash().to_string() != confirm {
        return Err(QuarantineError::Mismatch(plan));
    }
    if age_secs(now, stored.plan.created_at()) > MAX_PLAN_AGE_SECS {
        return Err(QuarantineError::TooOld(plan));
    }
    Ok(stored)
}

/// [`checked_plan`] for a cleanup: refused for a privileged process and for
/// an uninstall plan (ADR-0062).
fn checked_cleanup(
    index: &IndexStore,
    plan: PlanId,
    confirm: &str,
    now: Timestamp,
) -> Result<StoredPlan, QuarantineError> {
    refuse_privileged()?;
    let stored = checked_plan(index, plan, confirm, now)?;
    if lumen_application::uninstall::any_uninstall(&stored.decisions) {
        return Err(QuarantineError::NotACleanup(plan));
    }
    Ok(stored)
}

/// Executes plan `plan` from `db`, confirmed on `surface` by `confirm` (its
/// full hash). An uninstall plan is refused: it runs only through
/// [`crate::uninstall::uninstall`].
///
/// # Errors
///
/// A [`QuarantineError`] if the plan may not run or the store, database or
/// journal fails. Items that were skipped or failed are not errors; they are
/// reported in [`QuarantineReport::items`].
#[tracing::instrument(name = "quarantine", skip_all)]
pub fn quarantine(
    db: &Path,
    plan: PlanId,
    confirm: &str,
    surface: ConfirmationSurface,
    clock: &dyn Clock,
    ids: &dyn IdGenerator,
) -> Result<QuarantineReport, QuarantineError> {
    let now = clock.now();
    let mut index = IndexStore::new(open_index(db)?);
    let stored = checked_cleanup(&index, plan, confirm, now)?;
    // Held until the plan finishes; another cleanup in progress is waited for.
    let _lock = lock_ledger(db, true).map_err(QuarantineError::Lock)?;
    let confirmation = Confirmation {
        plan_hash: stored.plan.hash(),
        confirmed_at: now,
        surface,
    };
    index
        .apply_plan_event(plan, PlanEvent::Confirm { confirmation })
        .map_err(|error| match error {
            StoreError::PlanTransition(_) => QuarantineError::AlreadyConfirmed(plan),
            other => other.into(),
        })?;
    let quarantine_stores = stores()?;
    #[cfg(unix)]
    let mut warnings = match quarantine_stores.locations.first() {
        Some((base, names)) => prepare_store(base, names)?,
        None => Vec::new(),
    };
    #[cfg(not(unix))]
    let mut warnings = Vec::new();
    let executor = quarantine_stores.executor().ok_or(LocationError::NoHome)?;
    let mut ledger = LedgerStore::new(open_ledger(&db.with_file_name("ledger.db"))?);
    let mut journal = LedgerJournal::new(&mut ledger, clock);
    let resolved = reconcile(&executor, &mut journal)?.len();
    let device = device(db, ids)?;
    let assessment = prepare(db, clock)?;
    #[cfg(target_os = "macos")]
    let verifier = lumen_platform_macos::BulkEnumerator::new();
    #[cfg(all(unix, not(target_os = "macos")))]
    let verifier = lumen_scan::StdFsEnumerator;
    #[cfg(windows)]
    let verifier = lumen_platform_windows::HandleEnumerator;
    index.apply_plan_event(plan, PlanEvent::Start)?;
    let items = with_decision_probes(&assessment.apps, |probes| -> Result<_, QuarantineError> {
        let mut items = Vec::new();
        for (item, planned) in stored.plan.items().iter().zip(&stored.decisions) {
            // Each item is decided again from its own part of the scan.
            let subtree = assessment.subtree(&item.path)?;
            let with = Confirming {
                sources: assessment.sources(&subtree),
                roots: &assessment.roots,
                verifier: &verifier,
                probes: *probes,
                executor: &executor,
                judge: &NoopJudge,
                clock,
                ids,
                device,
                request: &|_| None,
            };
            let result = match block_on(confirm_item(item, planned, plan, &confirmation, &with)) {
                Confirmed::Skipped { reason, .. } => ItemResult::Skipped(reason),
                Confirmed::Ready(ready) => {
                    match quarantine_item(&executor, &mut journal, probes.in_use, &ready)? {
                        Outcome::Retained => ItemResult::Quarantined(ready.operation),
                        Outcome::Failed { reason, left } => ItemResult::Failed { reason, left },
                    }
                }
            };
            items.push(ItemReport {
                path: item.path.clone(),
                reclaim: item.reclaim.reclaimable_now,
                chosen_by: item.chosen_by,
                result,
            });
        }
        Ok(items)
    })??;
    record_items(&mut ledger, &items, &mut warnings);
    let all_succeeded = items
        .iter()
        .all(|i| matches!(i.result, ItemResult::Quarantined(_)));
    index.apply_plan_event(plan, PlanEvent::Finish { all_succeeded })?;
    let moved = ByteCount::new(
        items
            .iter()
            .filter(|i| matches!(i.result, ItemResult::Quarantined(_)))
            .map(|i| i.reclaim.get())
            .fold(0_u64, u64::saturating_add),
    );
    let count = |kind: fn(&ItemResult) -> bool| items.iter().filter(|i| kind(&i.result)).count();
    tracing::info!(
        items = items.len(),
        quarantined = count(|r| matches!(r, ItemResult::Quarantined(_))),
        skipped = count(|r| matches!(r, ItemResult::Skipped(_))),
        failed = count(|r| matches!(r, ItemResult::Failed { .. })),
        resolved,
        moved_bytes = moved.get(),
        warnings = warnings.len(),
        "plan executed"
    );
    Ok(QuarantineReport {
        plan,
        resolved,
        items,
        moved,
        warnings,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic, clippy::disallowed_methods)]
mod device_tests {
    use super::device;
    use lumen_application::ports::IdGenerator;
    use lumen_domain::{DeviceId, OperationId, PlanId, ScanId, SnapshotId};
    struct TempDir(std::path::PathBuf);
    impl TempDir {
        fn new() -> Self {
            let path =
                std::env::temp_dir().join(format!("lumen-device-test-{}", std::process::id()));
            match std::fs::create_dir(&path) {
                Ok(()) => Self(path),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                    static NEXT: std::sync::atomic::AtomicU64 =
                        std::sync::atomic::AtomicU64::new(0);
                    let path = std::env::temp_dir().join(format!(
                        "lumen-device-test-{}-{}",
                        std::process::id(),
                        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
                    ));
                    std::fs::create_dir(&path).unwrap();
                    Self(path)
                }
                Err(error) => panic!("could not create temporary test directory: {error}"),
            }
        }
        fn path(&self) -> &std::path::Path {
            &self.0
        }
    }
    impl Drop for TempDir {
        fn drop(&mut self) {
            std::fs::remove_dir_all(&self.0).unwrap();
        }
    }

    struct FixedIds;
    impl IdGenerator for FixedIds {
        fn scan_id(&self) -> ScanId {
            unreachable!()
        }
        fn snapshot_id(&self) -> SnapshotId {
            unreachable!()
        }
        fn plan_id(&self) -> PlanId {
            unreachable!()
        }
        fn operation_id(&self) -> OperationId {
            "018f0000-0000-7000-8000-000000000001".parse().unwrap()
        }
    }

    fn paths() -> (TempDir, std::path::PathBuf) {
        let dir = TempDir::new();
        let db = dir.path().join("db.sqlite");
        (dir, db)
    }

    #[test]
    fn missing_device_id_is_created_and_stable() {
        let (_dir, db) = paths();
        let ids = FixedIds;
        let first = device(&db, &ids).unwrap();
        assert_eq!(device(&db, &ids).unwrap(), first);
    }

    #[test]
    fn valid_device_id_is_read() {
        let (_dir, db) = paths();
        let id: DeviceId = "018f0000-0000-7000-8000-000000000002".parse().unwrap();
        let path = db.with_file_name("device-id");
        std::fs::write(&path, format!("{id}\n")).unwrap();
        assert_eq!(device(&db, &FixedIds).unwrap(), id);
    }

    #[test]
    fn non_utf8_device_id_is_rejected_without_replacement() {
        let (_dir, db) = paths();
        let path = db.with_file_name("device-id");
        let bytes = [0xff, 0xfe, 0xfd];
        std::fs::write(&path, bytes).unwrap();
        assert!(device(&db, &FixedIds).is_err());
        assert_eq!(std::fs::read(path).unwrap(), bytes);
    }
}
