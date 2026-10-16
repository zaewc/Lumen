//! `lumen quarantine`: executes exactly one confirmed plan (roadmap 10.14b,
//! ADR-0034, ADR-0015).
//!
//! Refuses unless the full plan hash matches the stored plan, the plan is at
//! most 24 hours old and still proposed, and the process is not running as
//! root or with an elevated Windows token (ADR-0005). Then, in order: interrupted operations from earlier runs are
//! reconciled; the plan is confirmed and started; every item is decided again
//! on fresh evidence and either skipped (reported as `REVIEW`) or quarantined
//! through the journaled, no-replace path; the plan finishes. Everything moved
//! stays restorable in the store.

use std::error::Error;
use std::path::Path;

use lumen_application::confirm::{Confirmed, Confirming, Skip, confirm_item};
use lumen_application::ports::{Clock, IdGenerator, NoopJudge};
use lumen_application::quarantine::{Outcome, quarantine_item};
use lumen_application::reconcile::reconcile;
use lumen_domain::{
    Confirmation, ConfirmationSurface, DeviceId, PlanEvent, PlanId, Timestamp, Verdict,
};
use lumen_fs_exec::Executor;
use lumen_store_sqlite::{
    IndexStore, LedgerJournal, LedgerStore, StoredPlan, open_index, open_ledger,
};

use lumen_app::{block_on, prepare, refuse_privileged, store_location, with_probes};

/// How long a plan may wait for its confirmation.
const MAX_PLAN_AGE_SECS: i64 = 24 * 60 * 60;

/// This installation's device ID, kept next to the database.
fn device(db: &Path, ids: &dyn IdGenerator) -> Result<DeviceId, Box<dyn Error>> {
    let path = db.with_file_name("device-id");
    if let Ok(text) = std::fs::read_to_string(&path) {
        return Ok(text.trim().parse()?);
    }
    // A fresh time-ordered ID; the operation ID generator gives one.
    let id: DeviceId = ids.operation_id().to_string().parse()?;
    std::fs::write(&path, format!("{id}\n"))?;
    Ok(id)
}

/// Creates the store if needed and asks the system not to back it up
/// (macOS, roadmap 9.2b). A failed exclusion is reported, not fatal. On
/// Windows the executor opens the store when it first moves an item, created
/// private and not content-indexed (ADR-0033).
#[cfg(unix)]
fn prepare_store(base: &Path, names: &[Vec<u8>]) -> Result<(), Box<dyn Error>> {
    use std::os::unix::fs::MetadataExt as _;

    use lumen_fs_exec::open_store;
    let volume = std::fs::metadata(base)?.dev();
    let names: Vec<&[u8]> = names.iter().map(Vec::as_slice).collect();
    let store = open_store(base, &names, volume)?;
    #[cfg(target_os = "macos")]
    if let Err(error) =
        lumen_platform_macos::exclude_from_backup(std::os::fd::AsFd::as_fd(store.handle()))
    {
        eprintln!("warning: the quarantine store could not be excluded from Time Machine: {error}");
    }
    let _store = store;
    Ok(())
}

fn age_secs(now: Timestamp, then: Timestamp) -> i64 {
    now.unix_seconds().saturating_sub(then.unix_seconds())
}

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

/// The stored plan, if it may be executed now with `confirm` (ADR-0034): not
/// privileged, the full hash matches, at most 24 hours old.
fn checked_plan(
    index: &IndexStore,
    plan: PlanId,
    confirm: &str,
    now: Timestamp,
) -> Result<StoredPlan, Box<dyn Error>> {
    refuse_privileged()?;
    let stored = index
        .plan(plan)?
        .ok_or_else(|| format!("plan {plan} does not exist; run `lumen plan` first"))?;
    if stored.plan.hash().to_string() != confirm {
        return Err(format!(
            "the confirmation does not match plan {plan}; nothing was moved (expected the full hash printed by `lumen plan`)"
        )
        .into());
    }
    if age_secs(now, stored.plan.created_at()) > MAX_PLAN_AGE_SECS {
        return Err("this plan is older than 24 hours; run `lumen plan` again".into());
    }
    Ok(stored)
}

/// Executes plan `plan` from `db`, confirmed by `confirm` (its full hash).
pub(crate) fn quarantine(
    db: &Path,
    plan: PlanId,
    confirm: &str,
    clock: &dyn Clock,
    ids: &dyn IdGenerator,
) -> Result<String, Box<dyn Error>> {
    let now = clock.now();
    let mut index = IndexStore::new(open_index(db)?);
    let stored = checked_plan(&index, plan, confirm, now)?;
    let plan_hash = stored.plan.hash();
    let confirmation = Confirmation {
        plan_hash,
        confirmed_at: now,
        surface: ConfirmationSurface::CommandLine,
    };
    index
        .apply_plan_event(plan, PlanEvent::Confirm { confirmation })
        .map_err(|error| match error {
            lumen_store_sqlite::StoreError::PlanTransition(_) => format!(
                "plan {plan} was already confirmed or executed; run `lumen plan` for a new one"
            )
            .into(),
            other => Box::new(other) as Box<dyn Error>,
        })?;
    let (base, names) = store_location()?;
    #[cfg(unix)]
    prepare_store(&base, &names)?;
    let executor = Executor::new(base, names);
    let mut ledger = LedgerStore::new(open_ledger(&db.with_file_name("ledger.db"))?);
    let mut journal = LedgerJournal::new(&mut ledger, clock);
    let mut lines = Vec::new();
    let resolved = reconcile(&executor, &mut journal)?;
    if !resolved.is_empty() {
        lines.push(format!(
            "Resolved {} interrupted operation(s) from earlier runs.",
            resolved.len()
        ));
    }
    let device = device(db, ids)?;
    let assessment = prepare(db, clock)?;
    #[cfg(target_os = "macos")]
    let verifier = lumen_platform_macos::BulkEnumerator::new();
    #[cfg(all(unix, not(target_os = "macos")))]
    let verifier = lumen_scan::StdFsEnumerator;
    #[cfg(windows)]
    let verifier = lumen_platform_windows::HandleEnumerator;
    index.apply_plan_event(plan, PlanEvent::Start)?;
    let mut all = true;
    let mut moved: u64 = 0;
    with_probes(|probes| -> Result<(), Box<dyn Error>> {
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
        for (item, planned) in stored.plan.items().iter().zip(&stored.decisions) {
            let path = item.path.display();
            match block_on(confirm_item(item, planned, plan, &confirmation, &with)) {
                Confirmed::Skipped { reason, .. } => {
                    all = false;
                    lines.push(format!(
                        "REVIEW      {path}  (skipped: {})",
                        skip_text(reason)
                    ));
                }
                Confirmed::Ready(ready) => {
                    match quarantine_item(&executor, &mut journal, probes.in_use, &ready)? {
                        Outcome::Retained => {
                            moved = moved.saturating_add(item.reclaim.reclaimable_now.get());
                            lines.push(format!(
                                "QUARANTINED {path}  (operation {})",
                                ready.operation
                            ));
                        }
                        Outcome::Failed { reason, left } => {
                            all = false;
                            lines.push(format!(
                                "FAILED      {path}  ({reason:?}; left in {left:?})"
                            ));
                        }
                    }
                }
            }
        }
        Ok(())
    })??;
    index.apply_plan_event(plan, PlanEvent::Finish { all_succeeded: all })?;
    lines.push(format!(
        "{moved} B now in quarantine (restorable; space is freed only when finalized)."
    ));
    Ok(lines.join("\n") + "\n")
}
