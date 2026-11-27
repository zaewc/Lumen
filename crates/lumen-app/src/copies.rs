//! Moving a copy of identical files the person chose into quarantine
//! (ADR-0065 as amended by ADR-0069), with [`lumen_application::copies`]'s
//! gate.
//!
//! A preview reads both copies again in full with the duplicate reader (which
//! checks each file is the one it was before and after reading), runs the
//! gate and stores a one-item plan chosen by the person, its decision bound
//! to both copies and their contents. The move does all of that again right
//! before the executor's usual move: the owner chose this check in the app
//! (2026-11-18), so `lumen-fs-exec` is unchanged. The copy that stays and the
//! contents' digest are given again by the host, which keeps the comparison;
//! the binding refuses anything but what the person confirmed.

use std::path::Path;

use lumen_application::CancelToken;
use lumen_application::copies::{
    Binding, CopyEvidence, FileFacts, GATE_VERSION, Ineligible, decision, gate, is_copy,
};
use lumen_application::duplicates::{FingerprintReader, HashStage};
use lumen_application::ports::{
    Clock, DirEnumerator as _, IdGenerator, InUseProbe, JournalError, PreparedItem,
    QuarantineExecutor,
};
use lumen_application::quarantine::{ConfirmedItem, Outcome, quarantine_item};
use lumen_application::reconcile::reconcile;
use lumen_domain::{
    ByteCount, Chooser, CleanupPlan, Confirmation, ConfirmationSurface, EntryKind, FilesystemEntry,
    ItemLocation, OperationId, PlanEvent, PlanId, PlannedItem, QuarantineFailure,
    QuarantineManifest, RawPath, Subject, Timestamp,
};
use lumen_store_sqlite::{
    IndexStore, LedgerJournal, LedgerStore, StoreError, open_index, open_ledger,
};

use crate::quarantine::{QuarantineError, checked_plan, device};
use crate::scan::enumerator;
use crate::store::{LocationError, PrivilegeError, Stores, lock_ledger};

/// Why a copy could not be previewed or moved. A refusal at the gate, a
/// changed copy and a failed move are not errors; they are results.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum CopyError {
    /// This platform cannot read contents to compare, or move copies yet.
    #[error("moving a copy is not supported on this platform")]
    Unsupported,
    /// The plan is not one copy's preview.
    #[error("plan {0} is not a copy's preview")]
    NotACopy(PlanId),
    /// Quarantine could not be prepared or the plan could not be used.
    #[error(transparent)]
    Quarantine(Box<QuarantineError>),
    /// The database failed.
    #[error("the index database failed: {0}")]
    Store(#[from] StoreError),
}

impl From<QuarantineError> for CopyError {
    fn from(error: QuarantineError) -> Self {
        Self::Quarantine(Box::new(error))
    }
}

impl From<JournalError> for CopyError {
    fn from(error: JournalError) -> Self {
        QuarantineError::Journal(error).into()
    }
}

/// A copy ready to confirm: its stored preview and what it shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CopyPreview {
    /// The preview's plan.
    pub plan: PlanId,
    /// Its full hash, the confirmation.
    pub hash: String,
    /// When it was made.
    pub created_at: Timestamp,
    /// The copy that moves.
    pub path: RawPath,
    /// The copy that stays.
    pub stays: RawPath,
    /// What moving it frees once removed for good.
    pub reclaim: ByteCount,
}

/// A preview, or every reason the copy cannot move.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Previewed {
    /// Stored, ready to confirm.
    Ready(CopyPreview),
    /// The gate refused.
    Refused(Vec<Ineligible>),
}

/// What a copy's move did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CopyResult {
    /// Held in quarantine as this operation, restorable.
    Quarantined(OperationId),
    /// The gate refused on fresh evidence; nothing moved.
    Refused(Vec<Ineligible>),
    /// Either copy, or the contents, changed since the preview; nothing moved.
    Changed,
    /// The executor did not move it, and says where it is.
    Failed {
        /// Why.
        reason: QuarantineFailure,
        /// Where it is.
        left: ItemLocation,
    },
}

/// An executed copy's preview.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CopyReport {
    /// The preview.
    pub plan: PlanId,
    /// Where the copy was.
    pub path: RawPath,
    /// What it frees once removed for good.
    pub reclaim: ByteCount,
    /// Interrupted operations from earlier runs that were resolved first.
    pub resolved: usize,
    /// What happened.
    pub result: CopyResult,
    /// Problems that did not stop it.
    pub warnings: Vec<String>,
}

/// Everything a copy's preview and move use besides the database: this
/// computer, or a fixture in tests.
pub(crate) struct Mover<'a> {
    /// Where quarantine stores live.
    pub(crate) stores: Stores,
    /// The live in-use probe, asked again right before the move.
    pub(crate) in_use: &'a dyn InUseProbe,
    /// Reads a file's contents to compare.
    pub(crate) reader: &'a dyn FingerprintReader,
    /// Whether a path is in a protected location.
    pub(crate) protected: &'a dyn Fn(&RawPath) -> bool,
    /// Refuses a privileged process.
    pub(crate) privileged: fn() -> Result<(), PrivilegeError>,
}

/// A file as it is now: its facts and its full digest, if it reads cleanly.
fn read_again(mover: &Mover<'_>, entry: &FilesystemEntry) -> Option<(FileFacts, [u8; 32])> {
    let now = enumerator().stat(&entry.path).ok()?;
    if now.identity() != entry.identity() {
        return None;
    }
    let digest = mover
        .reader
        .fingerprint(&now, HashStage::Full, &CancelToken::new())
        .ok()?;
    let facts = FileFacts {
        path: now.path.clone(),
        identity: now.identity().clone(),
        kind: now.kind,
        links: u64::from(now.size.link_count.get()),
        size: now.size.logical.get(),
    };
    Some((facts, digest))
}

/// The gate's evidence for `chosen` and `stays`, read again now, against the
/// compared `digest`; `None` when either is gone or another file now.
fn evidence(
    mover: &Mover<'_>,
    chosen: &FilesystemEntry,
    stays: &FilesystemEntry,
    digest: &[u8; 32],
    stays_outside_selection: bool,
) -> Option<CopyEvidence> {
    let (chosen_facts, chosen_digest) = read_again(mover, chosen)?;
    let (stays_facts, stays_digest) = read_again(mover, stays)?;
    Some(CopyEvidence {
        protected: (mover.protected)(&chosen_facts.path),
        same_contents: Some(chosen_digest == *digest && stays_digest == *digest),
        chosen: chosen_facts,
        stays: stays_facts,
        stays_outside_selection,
    })
}

/// Previews moving `chosen`, a copy whose contents `digest` the comparison
/// found also in `stays`, and stores the preview if the gate passes.
pub(crate) fn preview_with(
    mover: &Mover<'_>,
    db: &Path,
    chosen: &FilesystemEntry,
    stays: &FilesystemEntry,
    digest: &[u8; 32],
    clock: &dyn Clock,
    ids: &dyn IdGenerator,
) -> Result<Previewed, CopyError> {
    (mover.privileged)().map_err(QuarantineError::from)?;
    let Some(evidence) = evidence(mover, chosen, stays, digest, true) else {
        return Ok(Previewed::Refused(vec![Ineligible::ContentsDiffer]));
    };
    if let Err(refusal) = gate(&evidence) {
        return Ok(Previewed::Refused(refusal.reasons().to_vec()));
    }
    let hash = Binding {
        chosen: &evidence.chosen,
        stays: &evidence.stays,
        digest,
    }
    .hash();
    let reclaim = ByteCount::new(evidence.chosen.size);
    let item = PlannedItem {
        subject: Subject::File {
            identity: evidence.chosen.identity.clone(),
        },
        path: evidence.chosen.path.clone(),
        policy_version: GATE_VERSION,
        evidence_hash: hash,
        reclaim: lumen_domain::ReclaimEstimate {
            reclaimable_now: reclaim,
            ..lumen_domain::ReclaimEstimate::default()
        },
        chosen_by: Chooser::Person,
        scratch_change: None,
    };
    let plan = CleanupPlan::new(ids.plan_id(), clock.now(), vec![item]);
    IndexStore::new(open_index(db)?).save_plan(&plan, &[decision(hash)])?;
    Ok(Previewed::Ready(CopyPreview {
        plan: plan.id(),
        hash: plan.hash().to_string(),
        created_at: plan.created_at(),
        path: evidence.chosen.path,
        stays: evidence.stays.path,
        reclaim,
    }))
}

/// What the host gives again to move a copy: the copy that stays and the
/// contents' digest, from the comparison it keeps, the scan's real roots the
/// copy is under, and whether the copy that stays is outside everything
/// else confirmed.
pub(crate) struct Move<'a> {
    pub(crate) stays: &'a FilesystemEntry,
    pub(crate) digest: &'a [u8; 32],
    pub(crate) roots: &'a [RawPath],
    pub(crate) stays_outside_selection: bool,
}

/// Executes the copy's preview `plan`, confirmed on `surface` by `confirm`.
#[allow(
    clippy::too_many_arguments,
    reason = "the database, the plan, its confirmation, the host's facts and the clock and IDs"
)]
pub(crate) fn move_with(
    mover: &Mover<'_>,
    db: &Path,
    plan: PlanId,
    confirm: &str,
    given: &Move<'_>,
    surface: ConfirmationSurface,
    clock: &dyn Clock,
    ids: &dyn IdGenerator,
) -> Result<CopyReport, CopyError> {
    (mover.privileged)().map_err(QuarantineError::from)?;
    let now = clock.now();
    let mut index = IndexStore::new(open_index(db)?);
    let stored = checked_plan(&index, plan, confirm, now)?;
    let (planned, planned_decision) = match (stored.plan.items(), stored.decisions.as_slice()) {
        ([item], [decision])
            if is_copy(&stored.decisions)
                && item.policy_version == GATE_VERSION
                && item.chosen_by == Chooser::Person
                && item.evidence_hash == decision.evidence_hash() =>
        {
            (item.clone(), decision.clone())
        }
        _ => return Err(CopyError::NotACopy(plan)),
    };
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
    // On Windows the executor opens the store when it first moves an item.
    #[cfg(unix)]
    let mut warnings = match mover.stores.locations.first() {
        Some((base, names)) => crate::quarantine::prepare_store(base, names)?,
        None => Vec::new(),
    };
    #[cfg(not(unix))]
    let mut warnings = Vec::new();
    let executor = mover
        .stores
        .executor()
        .ok_or(QuarantineError::Location(LocationError::NoHome))?;
    let mut ledger = LedgerStore::new(open_ledger(&db.with_file_name("ledger.db"))?);
    let mut journal = LedgerJournal::new(&mut ledger, clock);
    // Every unfinished operation, the helper's too (ADR-0063 rule 6).
    let resolved = reconcile(&crate::routed::with_helper(executor.clone()), &mut journal)?.len();
    let device = device(db, ids)?;
    index.apply_plan_event(plan, PlanEvent::Start)?;
    let manifest = |entry: &FilesystemEntry, prepared: &PreparedItem, operation| {
        let manifest = QuarantineManifest {
            schema_version: QuarantineManifest::current_schema().ok()?,
            operation,
            plan,
            device,
            moved_at: clock.now(),
            original_path: planned.path.clone(),
            entry: entry.clone(),
            unix: executor.unix_ownership(prepared),
            decision: planned_decision.clone(),
            chosen_by: Chooser::Person,
            confirmed_at: confirmation.confirmed_at,
            jev_request: None,
        };
        serde_json::to_vec(&manifest).ok()
    };
    let result = execute_item(
        mover,
        &executor,
        &mut journal,
        &planned,
        given,
        manifest,
        ids,
    );
    let result = match result {
        Ok(result) => result,
        Err(error) => {
            let finish = PlanEvent::Finish {
                all_succeeded: false,
            };
            if let Err(stop) = index.apply_plan_event(plan, finish) {
                tracing::warn!(%stop, "the stopped copy's plan was not marked finished");
            }
            return Err(error);
        }
    };
    let reclaim = planned.reclaim.reclaimable_now;
    if let CopyResult::Quarantined(operation) = result {
        warnings.extend(record(&mut ledger, operation, reclaim));
    }
    let all_succeeded = matches!(result, CopyResult::Quarantined(_));
    index.apply_plan_event(plan, PlanEvent::Finish { all_succeeded })?;
    tracing::info!(quarantined = all_succeeded, resolved, "copy moved");
    Ok(CopyReport {
        plan,
        path: planned.path.clone(),
        reclaim,
        resolved,
        result,
        warnings,
    })
}

/// Whether the executor's record of `prepared` names `identity`. Only the
/// Unix executor can say; elsewhere it fails closed, so nothing moves.
fn recorded_is(prepared: &PreparedItem, identity: &lumen_domain::FileIdentity) -> bool {
    #[cfg(unix)]
    {
        lumen_fs_exec::recorded_identity(prepared).as_ref() == Some(identity)
    }
    #[cfg(not(unix))]
    {
        let _ = (prepared, identity);
        false
    }
}

/// Records what the held copy frees and that the person chose it, so it is
/// never removed automatically (ADR-0051).
fn record(ledger: &mut LedgerStore, operation: OperationId, reclaim: ByteCount) -> Vec<String> {
    let mut warnings = Vec::new();
    if let Err(error) = ledger.record_reclaim(operation, reclaim) {
        warnings.push(format!(
            "the size of the moved copy was not recorded: {error}"
        ));
    }
    if let Err(error) = ledger.record_chooser(operation, Chooser::Person) {
        warnings.push(format!(
            "who chose the moved copy was not recorded, so it waits for you to remove it: {error}"
        ));
    }
    warnings
}

/// Checks the copy and its pair again and moves it, or says why not.
fn execute_item(
    mover: &Mover<'_>,
    executor: &dyn QuarantineExecutor,
    journal: &mut dyn lumen_application::ports::QuarantineJournal,
    planned: &PlannedItem,
    given: &Move<'_>,
    manifest: impl Fn(&FilesystemEntry, &PreparedItem, OperationId) -> Option<Vec<u8>>,
    ids: &dyn IdGenerator,
) -> Result<CopyResult, CopyError> {
    let Subject::File { identity } = &planned.subject else {
        return Ok(CopyResult::Changed);
    };
    let Some(chosen) = enumerator()
        .stat(&planned.path)
        .ok()
        .filter(|entry| entry.identity() == identity)
    else {
        return Ok(CopyResult::Changed);
    };
    let Some(evidence) = evidence(
        mover,
        &chosen,
        given.stays,
        given.digest,
        given.stays_outside_selection,
    ) else {
        return Ok(CopyResult::Changed);
    };
    if let Err(refusal) = gate(&evidence) {
        return Ok(CopyResult::Refused(refusal.reasons().to_vec()));
    }
    let hash = Binding {
        chosen: &evidence.chosen,
        stays: &evidence.stays,
        digest: given.digest,
    }
    .hash();
    if hash != planned.evidence_hash {
        return Ok(CopyResult::Changed);
    }
    let Some((root, relative)) = lumen_application::confirm::split(&planned.path, given.roots)
    else {
        return Ok(CopyResult::Changed);
    };
    let Ok(prepared) = executor.prepare(&root, &relative) else {
        return Ok(CopyResult::Changed);
    };
    // What the executor recorded must be the confirmed copy: it moves the
    // file its record names, not whatever the path names later.
    if !recorded_is(&prepared, identity) {
        return Ok(CopyResult::Changed);
    }
    // Observed after the executor recorded it: what it recorded is the
    // planned copy, or the executor's own check refuses the move.
    let Some(entry) = enumerator()
        .stat(&planned.path)
        .ok()
        .filter(|e| e.identity() == identity && e.kind == EntryKind::File)
    else {
        return Ok(CopyResult::Changed);
    };
    let operation = ids.operation_id();
    let Some(manifest) = manifest(&entry, &prepared, operation) else {
        return Ok(CopyResult::Changed);
    };
    let ready = ConfirmedItem {
        operation,
        item: prepared,
        path: planned.path.clone(),
        identity: identity.clone(),
        manifest,
        // A file someone may have open is never moved on a partial answer.
        partial_in_use_accepted: false,
    };
    Ok(
        match quarantine_item(executor, journal, mover.in_use, &ready)
            .map_err(QuarantineError::from)?
        {
            Outcome::Retained => CopyResult::Quarantined(operation),
            Outcome::Failed { reason, left } => CopyResult::Failed { reason, left },
        },
    )
}

/// Runs `run` with this computer's stores, live in-use probe, content reader
/// and protected locations.
fn on_this_computer<T>(
    run: impl FnOnce(&Mover<'_>) -> Result<T, CopyError>,
) -> Result<T, CopyError> {
    // Only the Unix executor can show which file it recorded (see
    // [`recorded_is`]); elsewhere copies are not moved yet.
    if cfg!(not(unix)) {
        return Err(CopyError::Unsupported);
    }
    let stores = crate::store::stores().map_err(QuarantineError::Location)?;
    let context = crate::duplicates::context();
    let protected = |path: &RawPath| lumen_policy::protected_location(path, &context).is_some();
    crate::duplicates::with_reader(|reader| {
        crate::assess::with_probes(|probes| {
            run(&Mover {
                stores,
                in_use: probes.in_use,
                reader,
                protected: &protected,
                privileged: crate::store::refuse_privileged,
            })
        })
        .map_err(|error| CopyError::from(QuarantineError::from(error)))?
    })
    .map_err(|_| CopyError::Unsupported)?
}

/// Previews moving `chosen`, a copy the comparison found with the same
/// contents (`digest`) as `stays`, and stores the preview if the gate passes.
///
/// # Errors
///
/// [`CopyError::Unsupported`] where contents cannot be read, or a store or
/// database error. A refusal is [`Previewed::Refused`].
#[tracing::instrument(name = "copy_preview", skip_all)]
pub fn preview(
    db: &Path,
    chosen: &FilesystemEntry,
    stays: &FilesystemEntry,
    digest: &[u8; 32],
    clock: &dyn Clock,
    ids: &dyn IdGenerator,
) -> Result<Previewed, CopyError> {
    on_this_computer(|mover| preview_with(mover, db, chosen, stays, digest, clock, ids))
}

/// Moves the copy of preview `plan`, confirmed on `surface` by `confirm` (its
/// full hash), after reading it and `stays` again (ADR-0069). `roots` are the
/// scan's real roots the copy is under.
///
/// # Errors
///
/// A [`CopyError`] if the preview may not run, or the store, database or
/// journal fails. A refusal, a changed copy and a failed move are the
/// report's [`CopyReport::result`].
#[allow(
    clippy::too_many_arguments,
    reason = "the database, the plan, its confirmation, the host's facts and the clock and IDs"
)]
#[tracing::instrument(name = "copy_move", skip_all)]
pub fn move_copy(
    db: &Path,
    plan: PlanId,
    confirm: &str,
    stays: &FilesystemEntry,
    digest: &[u8; 32],
    roots: &[RawPath],
    stays_outside_selection: bool,
    surface: ConfirmationSurface,
    clock: &dyn Clock,
    ids: &dyn IdGenerator,
) -> Result<CopyReport, CopyError> {
    let given = Move {
        stays,
        digest,
        roots,
        stays_outside_selection,
    };
    on_this_computer(|mover| move_with(mover, db, plan, confirm, &given, surface, clock, ids))
}

#[cfg(all(test, any(target_os = "macos", target_os = "linux")))]
mod tests {
    use lumen_application::ports::InUse;
    use lumen_domain::FileIdentity;
    use lumen_testkit::{Fixture, Node};

    use super::*;
    use crate::{SystemClock, UuidV7Ids};

    struct Free;

    impl InUseProbe for Free {
        fn probe(&self, _: &RawPath, _: &FileIdentity) -> InUse {
            InUse::No
        }
    }

    struct Setup {
        fixture: Fixture,
        db: std::path::PathBuf,
        root: RawPath,
        chosen: FilesystemEntry,
        stays: FilesystemEntry,
        digest: [u8; 32],
    }

    fn setup() -> Result<Setup, Box<dyn std::error::Error>> {
        let contents = b"the same tutorial video, twice".repeat(64);
        let fixture = Fixture::build(&[
            Node::File {
                path: "files/Downloads/tutorial.mov".into(),
                contents: contents.clone(),
            },
            Node::File {
                path: "files/Movies/tutorial.mov".into(),
                contents,
            },
        ])?;
        let real = std::fs::canonicalize(fixture.path("files")?)?;
        let root = crate::raw_path(&real)?;
        let stat = |rest: &str| -> Result<FilesystemEntry, Box<dyn std::error::Error>> {
            Ok(enumerator().stat(&crate::raw_path(&real.join(rest))?)?)
        };
        let chosen = stat("Downloads/tutorial.mov")?;
        let stays = stat("Movies/tutorial.mov")?;
        let digest = crate::duplicates::with_reader(|reader| {
            reader.fingerprint(&stays, HashStage::Full, &CancelToken::new())
        })?
        .map_err(|failure| format!("the fixture could not be read: {failure:?}"))?;
        Ok(Setup {
            db: fixture.path("index.db")?,
            fixture,
            root,
            chosen,
            stays,
            digest,
        })
    }

    fn run<T>(
        setup: &Setup,
        go: impl FnOnce(&Mover<'_>) -> Result<T, CopyError>,
    ) -> Result<T, Box<dyn std::error::Error>> {
        let base = setup.fixture.path("home/Library/Application Support")?;
        std::fs::create_dir_all(&base)?;
        // As the real stores are: the real path (a temporary folder may be behind a link).
        let base = std::fs::canonicalize(base)?;
        let never = |_: &RawPath| false;
        Ok(crate::duplicates::with_reader(|reader| {
            go(&Mover {
                stores: Stores {
                    locations: vec![(base, vec![b"Lumen".to_vec(), b"Quarantine".to_vec()])],
                    former: Vec::new(),
                    volumes: None,
                    single_user: false,
                },
                in_use: &Free,
                reader,
                protected: &never,
                privileged: || Ok(()),
            })
        })??)
    }

    fn ready(previewed: Previewed) -> Result<CopyPreview, Box<dyn std::error::Error>> {
        match previewed {
            Previewed::Ready(preview) => Ok(preview),
            Previewed::Refused(reasons) => Err(format!("refused: {reasons:?}").into()),
        }
    }

    #[test]
    fn a_chosen_copy_moves_and_the_other_stays() -> Result<(), Box<dyn std::error::Error>> {
        let s = setup()?;
        let preview = ready(run(&s, |m| {
            preview_with(
                m,
                &s.db,
                &s.chosen,
                &s.stays,
                &s.digest,
                &SystemClock,
                &UuidV7Ids,
            )
        })?)?;
        let given = Move {
            stays: &s.stays,
            digest: &s.digest,
            roots: std::slice::from_ref(&s.root),
            stays_outside_selection: true,
        };
        let report = run(&s, |m| {
            move_with(
                m,
                &s.db,
                preview.plan,
                &preview.hash,
                &given,
                ConfirmationSurface::CommandLine,
                &SystemClock,
                &UuidV7Ids,
            )
        })?;
        assert!(
            matches!(report.result, CopyResult::Quarantined(_)),
            "{report:?}"
        );
        assert!(
            enumerator().stat(&s.chosen.path).is_err(),
            "the chosen copy moved"
        );
        assert!(
            enumerator().stat(&s.stays.path).is_ok(),
            "the other copy stays"
        );
        Ok(())
    }

    #[test]
    fn nothing_moves_when_nothing_would_stay_or_contents_changed()
    -> Result<(), Box<dyn std::error::Error>> {
        let s = setup()?;
        let preview = ready(run(&s, |m| {
            preview_with(
                m,
                &s.db,
                &s.chosen,
                &s.stays,
                &s.digest,
                &SystemClock,
                &UuidV7Ids,
            )
        })?)?;
        let inside = Move {
            stays: &s.stays,
            digest: &s.digest,
            roots: std::slice::from_ref(&s.root),
            stays_outside_selection: false,
        };
        let report = run(&s, |m| {
            move_with(
                m,
                &s.db,
                preview.plan,
                &preview.hash,
                &inside,
                ConfirmationSurface::CommandLine,
                &SystemClock,
                &UuidV7Ids,
            )
        })?;
        assert_eq!(
            report.result,
            CopyResult::Refused(vec![Ineligible::NothingStays])
        );
        assert!(enumerator().stat(&s.chosen.path).is_ok());

        // Another preview, then a different digest than the one confirmed.
        let again = ready(run(&s, |m| {
            preview_with(
                m,
                &s.db,
                &s.chosen,
                &s.stays,
                &s.digest,
                &SystemClock,
                &UuidV7Ids,
            )
        })?)?;
        let other = [9; 32];
        let wrong = Move {
            stays: &s.stays,
            digest: &other,
            roots: std::slice::from_ref(&s.root),
            stays_outside_selection: true,
        };
        let report = run(&s, |m| {
            move_with(
                m,
                &s.db,
                again.plan,
                &again.hash,
                &wrong,
                ConfirmationSurface::CommandLine,
                &SystemClock,
                &UuidV7Ids,
            )
        })?;
        assert_eq!(
            report.result,
            CopyResult::Refused(vec![Ineligible::ContentsDiffer])
        );
        assert!(enumerator().stat(&s.chosen.path).is_ok(), "nothing moved");
        Ok(())
    }

    #[test]
    fn a_copy_differing_from_the_comparison_is_refused_at_preview()
    -> Result<(), Box<dyn std::error::Error>> {
        let s = setup()?;
        let previewed = run(&s, |m| {
            preview_with(
                m,
                &s.db,
                &s.chosen,
                &s.stays,
                &[0; 32],
                &SystemClock,
                &UuidV7Ids,
            )
        })?;
        assert_eq!(
            previewed,
            Previewed::Refused(vec![Ineligible::ContentsDiffer])
        );
        let same = run(&s, |m| {
            preview_with(
                m,
                &s.db,
                &s.chosen,
                &s.chosen,
                &s.digest,
                &SystemClock,
                &UuidV7Ids,
            )
        })?;
        assert!(matches!(same, Previewed::Refused(r) if r.contains(&Ineligible::SameFile)));

        // A copy's preview never runs as a clean-up.
        let preview = ready(run(&s, |m| {
            preview_with(
                m,
                &s.db,
                &s.chosen,
                &s.stays,
                &s.digest,
                &SystemClock,
                &UuidV7Ids,
            )
        })?)?;
        let cleanup = crate::quarantine::quarantine(
            &s.db,
            preview.plan,
            &preview.hash,
            ConfirmationSurface::CommandLine,
            &SystemClock,
            &UuidV7Ids,
        );
        assert!(
            matches!(cleanup, Err(QuarantineError::NotACleanup(_))),
            "{cleanup:?}"
        );
        assert!(enumerator().stat(&s.chosen.path).is_ok());
        Ok(())
    }
}
