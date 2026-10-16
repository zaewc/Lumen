//! Executing exactly one confirmed plan (roadmap 10.14b, 11.4; ADR-0034,
//! ADR-0015).
//!
//! Refuses unless the process is not privileged (ADR-0005), the plan exists,
//! the confirmation is its full hash, and it is at most 24 hours old and
//! still proposed. Then, in order: interrupted operations from earlier runs
//! are reconciled; the plan is confirmed and started; every item is decided
//! again on fresh evidence and either skipped or quarantined through the
//! journaled, no-replace path; the plan finishes. Everything moved stays
//! restorable in the store.

use std::path::Path;

use lumen_application::confirm::{Confirmed, Confirming, Skip, confirm_item};
use lumen_application::ports::{Clock, IdGenerator, JournalError, NoopJudge};
use lumen_application::quarantine::{Outcome, quarantine_item};
use lumen_application::reconcile::reconcile;
use lumen_domain::{
    ByteCount, Confirmation, ConfirmationSurface, DeviceId, ItemLocation, OperationId, PlanEvent,
    PlanId, QuarantineFailure, RawPath, Timestamp,
};
use lumen_fs_exec::Executor;
use lumen_store_sqlite::{
    IndexStore, LedgerJournal, LedgerStore, StoreError, StoredPlan, open_index, open_ledger,
};

use crate::assess::{AssessError, block_on, prepare, with_probes};
use crate::store::{LocationError, PrivilegeError, refuse_privileged, store_location};

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
fn device(db: &Path, ids: &dyn IdGenerator) -> Result<DeviceId, QuarantineError> {
    let path = db.with_file_name("device-id");
    if let Ok(text) = std::fs::read_to_string(&path) {
        return text.trim().parse().map_err(|_| {
            QuarantineError::Device(format!("{} is not a device ID", path.display()))
        });
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
fn prepare_store(base: &Path, names: &[Vec<u8>]) -> Result<Vec<String>, QuarantineError> {
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

fn age_secs(now: Timestamp, then: Timestamp) -> i64 {
    now.unix_seconds().saturating_sub(then.unix_seconds())
}

/// The stored plan, if it may be executed now with `confirm` (ADR-0034): not
/// privileged, the full hash matches, at most 24 hours old.
fn checked_plan(
    index: &IndexStore,
    plan: PlanId,
    confirm: &str,
    now: Timestamp,
) -> Result<StoredPlan, QuarantineError> {
    refuse_privileged()?;
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

/// Executes plan `plan` from `db`, confirmed on `surface` by `confirm` (its
/// full hash).
///
/// # Errors
///
/// A [`QuarantineError`] if the plan may not run or the store, database or
/// journal fails. Items that were skipped or failed are not errors; they are
/// reported in [`QuarantineReport::items`].
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
    let stored = checked_plan(&index, plan, confirm, now)?;
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
    let (base, names) = store_location()?;
    #[cfg(unix)]
    let warnings = prepare_store(&base, &names)?;
    #[cfg(not(unix))]
    let warnings = Vec::new();
    let executor = Executor::new(base, names);
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
    let items = with_probes(|probes| -> Result<Vec<ItemReport>, QuarantineError> {
        let with = Confirming {
            sources: assessment.sources(),
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
        let mut items = Vec::new();
        for (item, planned) in stored.plan.items().iter().zip(&stored.decisions) {
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
                result,
            });
        }
        Ok(items)
    })??;
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
    Ok(QuarantineReport {
        plan,
        resolved,
        items,
        moved,
        warnings,
    })
}
