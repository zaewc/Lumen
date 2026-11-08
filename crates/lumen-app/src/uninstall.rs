//! Uninstalling a user-owned macOS application by moving it into quarantine
//! (ADR-0062).
//!
//! [`preview`] finds the one bundle the software list names by its key
//! (`MacApp:<bundle id>`), gathers fresh read-only evidence about it, and runs
//! ADR-0062's gate ([`lumen_application::uninstall::gate`]). If it passes, the
//! preview is stored as a one-item plan chosen by the person, whose hash the
//! person confirms (ADR-0034). [`uninstall`] executes exactly that plan: it
//! refuses a privileged process, a wrong, expired or reused confirmation and
//! any cleanup plan, then locks the ledger, settles interrupted operations,
//! gathers the evidence again and runs the gate again, refuses an app that
//! changed since its preview, and moves the bundle through the journaled,
//! no-replace quarantine path, exactly like a cleanup item. It is then held in
//! Quarantine, restorable until the person removes it for good; it is never
//! removed automatically (ADR-0051).
//!
//! Nothing reads file contents: the evidence is the inventories, directory
//! metadata, the process table and a listing of the bundle for its size. The
//! cleanup policy and AI review take no part (ADR-0062 rule 3). Only macOS
//! applications qualify; elsewhere both calls say so.

use std::path::Path;

use lumen_application::ports::{Clock, IdGenerator, JournalError};
use lumen_application::uninstall::Ineligible;
use lumen_domain::{
    ByteCount, ConfirmationSurface, ItemLocation, OperationId, PlanHash, PlanId, QuarantineFailure,
    RawPath, ReclaimEstimate, Timestamp, UntrustedText,
};

use crate::quarantine::QuarantineError;
use crate::store::PrivilegeError;

/// Why no preview was made, or a preview was not executed. Nothing was moved
/// in any of these cases.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum UninstallError {
    /// Only macOS applications are uninstalled into quarantine.
    #[error("uninstalling into quarantine is available only for macOS applications")]
    Unsupported,
    /// The application list has no bundle with that key.
    #[error("no installed application has that key; list the software again")]
    UnknownApp,
    /// More than one bundle has that key, so which one is meant is unknown.
    #[error("more than one application has that key; uninstall it in Finder")]
    Ambiguous,
    /// The plan is not an uninstall preview.
    #[error("plan {0} is not an uninstall preview; nothing was moved")]
    NotAnUninstall(PlanId),
    /// The plan could not be executed, or the store, database or journal
    /// failed.
    #[error(transparent)]
    Quarantine(Box<QuarantineError>),
}

impl From<QuarantineError> for UninstallError {
    fn from(error: QuarantineError) -> Self {
        Self::Quarantine(Box::new(error))
    }
}

impl From<PrivilegeError> for UninstallError {
    fn from(error: PrivilegeError) -> Self {
        QuarantineError::Privileged(error).into()
    }
}

impl From<JournalError> for UninstallError {
    fn from(error: JournalError) -> Self {
        QuarantineError::Journal(error).into()
    }
}

impl From<lumen_store_sqlite::StoreError> for UninstallError {
    fn from(error: lumen_store_sqlite::StoreError) -> Self {
        QuarantineError::Store(error).into()
    }
}

/// The software list's key for a macOS application: `MacApp:` and its bundle
/// identifier, display-escaped, as `lumen-api` lists it.
pub fn key(bundle_id: &UntrustedText) -> String {
    format!("MacApp:{}", bundle_id.display())
}

/// The application a preview is about, as observed now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UninstallApp {
    /// The software list's key.
    pub key: String,
    /// Its bundle identifier.
    pub bundle_id: UntrustedText,
    /// Its display name.
    pub name: Option<UntrustedText>,
    /// Its version.
    pub version: Option<UntrustedText>,
    /// Where the bundle is.
    pub path: RawPath,
    /// What moving it frees once it is removed for good.
    pub reclaim: ReclaimEstimate,
    /// Every folder inside it was listed; otherwise `reclaim` is a lower
    /// bound.
    pub size_complete: bool,
}

/// A stored preview the person can confirm.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Confirmable {
    /// The preview's ID (a plan ID).
    pub plan: PlanId,
    /// Its hash: the confirmation.
    pub hash: PlanHash,
    /// When it was made; it expires 24 hours later.
    pub created_at: Timestamp,
}

/// What uninstalling an application would do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UninstallPreview {
    /// The application.
    pub app: UninstallApp,
    /// The preview to confirm, or every reason it cannot be uninstalled this
    /// way (nothing was stored then).
    pub ready: Result<Confirmable, Vec<Ineligible>>,
}

/// What became of the application.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UninstallResult {
    /// It is in Quarantine as `operation`, restorable.
    Quarantined(OperationId),
    /// The gate refused it now; it was not moved.
    Refused(Vec<Ineligible>),
    /// It is not what its preview showed (gone, replaced, changed size or
    /// version, or listed elsewhere); it was not moved. Preview it again.
    Changed,
    /// The move failed; `left` says where the bundle is.
    Failed {
        /// Why.
        reason: QuarantineFailure,
        /// Where it is.
        left: ItemLocation,
    },
}

/// What an uninstall did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UninstallReport {
    /// The preview executed.
    pub plan: PlanId,
    /// Interrupted operations from earlier runs that were resolved first.
    pub resolved: usize,
    /// The bundle, as previewed.
    pub path: RawPath,
    /// What it frees once removed for good, as previewed.
    pub reclaim: ByteCount,
    /// What happened.
    pub result: UninstallResult,
    /// Problems that did not stop it, such as a store that could not be
    /// excluded from backups.
    pub warnings: Vec<String>,
}

/// Previews uninstalling the application listed under `key`, and stores the
/// preview if ADR-0062's gate passes.
///
/// # Errors
///
/// [`UninstallError::Unsupported`] off macOS, [`UninstallError::UnknownApp`]
/// or [`UninstallError::Ambiguous`] if `key` names no bundle or several, or
/// a database error.
#[tracing::instrument(name = "uninstall_preview", skip_all)]
pub fn preview(
    db: &Path,
    key: &str,
    clock: &dyn Clock,
    ids: &dyn IdGenerator,
) -> Result<UninstallPreview, UninstallError> {
    #[cfg(target_os = "macos")]
    {
        let software = mac::Software;
        crate::assess::with_probes(|probes| {
            mac::computer(&software, probes.in_use)
                .and_then(|computer| unix::preview_with(&computer, db, key, clock, ids))
        })
        .map_err(QuarantineError::from)?
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (db, key, clock, ids);
        Err(UninstallError::Unsupported)
    }
}

/// Executes the uninstall preview `plan` from `db`, confirmed on `surface` by
/// `confirm` (its full hash).
///
/// # Errors
///
/// An [`UninstallError`] if the preview may not run, or the store, database
/// or journal fails. A refusal at the gate, a changed app and a failed move
/// are not errors; they are the report's [`UninstallReport::result`].
#[tracing::instrument(name = "uninstall", skip_all)]
pub fn uninstall(
    db: &Path,
    plan: PlanId,
    confirm: &str,
    surface: ConfirmationSurface,
    clock: &dyn Clock,
    ids: &dyn IdGenerator,
) -> Result<UninstallReport, UninstallError> {
    #[cfg(target_os = "macos")]
    {
        let software = mac::Software;
        crate::assess::with_probes(|probes| {
            mac::computer(&software, probes.in_use).and_then(|computer| {
                unix::uninstall_with(&computer, db, plan, confirm, surface, clock, ids)
            })
        })
        .map_err(QuarantineError::from)?
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (db, plan, confirm, surface, clock, ids);
        Err(UninstallError::Unsupported)
    }
}

#[cfg(unix)]
#[cfg_attr(
    not(target_os = "macos"),
    allow(dead_code, reason = "only macOS uses it outside tests")
)]
mod unix {
    //! The parts that read Unix metadata and move the bundle.

    use std::ops::ControlFlow;
    use std::os::unix::fs::MetadataExt as _;
    use std::path::{Path, PathBuf};
    use std::time::{Duration, Instant};

    use lumen_application::CancelToken;
    use lumen_application::confirm::split;
    use lumen_application::ports::{DirEnumerator as _, OwnershipProbe as _, QuarantineExecutor};
    use lumen_application::quarantine::{ConfirmedItem, Outcome, quarantine_item};
    use lumen_application::reconcile::reconcile;
    use lumen_application::software_links::link_services;
    use lumen_application::uninstall::{
        Binding, BundleEvidence, GATE_VERSION, decision, gate, is_uninstall,
    };
    use std::collections::{BTreeMap, BTreeSet};

    use lumen_application::ports::{InUseProbe, InventoryReport, PreparedItem, QuarantineJournal};
    use lumen_application::uninstall::HelperFolder;
    use lumen_domain::{
        AppIdScheme, Application, Chooser, CleanupPlan, Confirmation, ConfirmationSurface,
        EntryKind, FilesystemEntry, OperationId, PlanEvent, PlanId, PlannedItem, PolicyDecision,
        QuarantineManifest, RawPath, ReclaimEstimate, SourceStatus, Subject, UntrustedText,
    };
    use lumen_scan::{ScanItem, UnixOwnershipProbe, scan as run_scan};
    use lumen_store_sqlite::{
        IndexStore, LedgerJournal, LedgerStore, StoreError, open_index, open_ledger,
    };

    use super::{
        ByteCount, Confirmable, UninstallApp, UninstallError, UninstallPreview, UninstallReport,
        UninstallResult, key,
    };
    use crate::quarantine::{QuarantineError, checked_plan, device, prepare_store};
    use crate::scan::{enumerator, excluding_stores};
    use crate::store::{LocationError, PrivilegeError, Stores, lock_ledger};

    /// Where an uninstall looks and what it asks: this Mac, or a fixture in
    /// tests.
    pub(super) trait Software {
        /// The application inventory.
        fn applications(&self) -> InventoryReport;
        /// The background items.
        fn services(&self) -> InventoryReport;
        /// Whether a process runs an executable inside `bundle` (ADR-0059).
        fn running(&self, bundle: &RawPath) -> Option<bool>;
    }

    /// Everything an uninstall uses besides the database.
    pub(super) struct Computer<'a> {
        /// Where an uninstallable bundle may lie directly.
        pub(super) folders: Vec<PathBuf>,
        /// Where quarantine stores live.
        pub(super) stores: Stores,
        /// The inventories and the process table.
        pub(super) software: &'a dyn Software,
        /// The live in-use probe, asked again right before the move.
        pub(super) in_use: &'a dyn InUseProbe,
        /// Refuses a privileged process.
        pub(super) privileged: fn() -> Result<(), PrivilegeError>,
    }

    /// The one application `key` names, with its one location.
    fn find<'a>(apps: &'a [Application], key: &str) -> Result<&'a Application, UninstallError> {
        let mut found = apps
            .iter()
            .filter(|app| bundle_id(app).is_some_and(|id| self::key(id) == key));
        let app = found.next().ok_or(UninstallError::UnknownApp)?;
        if found.next().is_some() || app.install_locations.len() != 1 {
            return Err(UninstallError::Ambiguous);
        }
        Ok(app)
    }

    fn bundle_id(app: &Application) -> Option<&UntrustedText> {
        app.ids
            .iter()
            .find(|id| id.scheme == AppIdScheme::BundleId)
            .map(|id| &id.value)
    }

    /// Whether each helper folder exists below `bundle`, from metadata only,
    /// without following links: a `Contents` or `Contents/Library` that is a
    /// link, or any answer other than "exists" or "not found", leaves every
    /// folder unknown.
    fn helpers(bundle: &Path) -> BTreeMap<HelperFolder, Option<bool>> {
        let all = |answer| HelperFolder::ALL.map(|f| (f, answer)).into();
        let mut parent = bundle.to_path_buf();
        for name in ["Contents", "Library"] {
            parent.push(name);
            match std::fs::symlink_metadata(&parent) {
                Ok(meta) if meta.is_dir() => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    return all(Some(false));
                }
                Ok(_) | Err(_) => return all(None),
            }
        }
        HelperFolder::ALL
            .map(|folder| {
                let answer = match std::fs::symlink_metadata(parent.join(folder.name())) {
                    Ok(_) => Some(true),
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => Some(false),
                    Err(_) => None,
                };
                (folder, answer)
            })
            .into()
    }

    /// How long listing a bundle for its size may take.
    const MEASURE_LIMIT: Duration = Duration::from_secs(30);

    fn native(path: &RawPath) -> PathBuf {
        use std::os::unix::ffi::OsStrExt as _;
        PathBuf::from(std::ffi::OsStr::from_bytes(path.as_bytes()))
    }

    /// What the bundle frees, from a listing that follows no link, enters no
    /// store and stops after [`MEASURE_LIMIT`]; and whether it was complete.
    fn measure(path: &RawPath, stores: &[PathBuf]) -> (ReclaimEstimate, bool) {
        let Ok(config) = excluding_stores(&[native(path)], stores) else {
            return (ReclaimEstimate::default(), false);
        };
        let started = Instant::now();
        let mut entries: Vec<FilesystemEntry> = Vec::new();
        let mut complete = true;
        run_scan(
            &enumerator(),
            std::slice::from_ref(path),
            &config,
            &CancelToken::new(),
            |item| {
                if started.elapsed() > MEASURE_LIMIT {
                    complete = false;
                    return ControlFlow::Break(());
                }
                match item {
                    ScanItem::Entry(entry) => entries.push(entry),
                    ScanItem::Listed { outcome, .. } => {
                        complete &= outcome.complete
                            && outcome.access == lumen_domain::AccessState::Readable;
                    }
                    ScanItem::Skipped { .. } => complete = false,
                    _ => {}
                }
                ControlFlow::Continue(())
            },
        );
        let reclaim = ReclaimEstimate::for_selection(entries.iter().map(|e| &e.size));
        (reclaim, complete)
    }

    /// Programs whose job is to start other applications (`open -a X.app`).
    /// Which app a background item running one starts is in its arguments,
    /// which the inventory does not keep, so such an item may start any app.
    const LAUNCHERS: [&[u8]; 1] = [b"/usr/bin/open"];

    /// `path` lowercased in ASCII, followed by `/`: a prefix for anything
    /// inside it, however a case-insensitive volume lets it be spelled.
    fn folded_prefix(path: &[u8]) -> Vec<u8> {
        let mut prefix = path.to_ascii_lowercase();
        if !prefix.ends_with(b"/") {
            prefix.push(b'/');
        }
        prefix
    }

    /// How many background items may run `app` (ADR-0062), or `None` when
    /// that cannot be known.
    ///
    /// The list's links ([`link_services`]) only ever miss a link, which is
    /// right for display and wrong for this gate, where a missed link passes.
    /// So this counts those links and, on top, any item whose program lies
    /// inside the bundle ignoring letter case, whose program is a link that
    /// resolves inside it, or whose label starts with the bundle ID ignoring
    /// case. An item running a launcher ([`LAUNCHERS`]) may start any app, so
    /// the answer is unknown. Counting too many only refuses.
    fn linked(
        apps: &[Application],
        app: &Application,
        services: &InventoryReport,
    ) -> Option<usize> {
        if services.status != SourceStatus::Complete {
            return None;
        }
        let all: Vec<&_> = apps.iter().collect();
        let items: Vec<&_> = services.services.iter().collect();
        let target = apps.iter().position(|a| std::ptr::eq(a, app));
        let listed: BTreeSet<usize> = link_services(&all, &items)
            .iter()
            .filter(|link| Some(link.application) == target)
            .map(|link| link.service)
            .collect();
        let bundles: Vec<Vec<u8>> = app
            .install_locations
            .iter()
            .flat_map(|path| {
                let canonical = std::fs::canonicalize(native(path))
                    .ok()
                    .and_then(|p| raw(&p));
                [Some(path.clone()), canonical]
            })
            .flatten()
            .map(|path| folded_prefix(path.as_bytes()))
            .collect();
        let ids: Vec<String> = bundle_id(app)
            .map(|id| id.raw().to_ascii_lowercase())
            .filter(|id| !id.is_empty())
            .into_iter()
            .collect();
        let mut count = 0;
        for (index, service) in services.services.iter().enumerate() {
            let program = service.program.as_ref();
            if program.is_some_and(|p| LAUNCHERS.contains(&p.as_bytes())) {
                return None;
            }
            let inside = |path: &RawPath| {
                let folded = path.as_bytes().to_ascii_lowercase();
                bundles.iter().any(|bundle| folded.starts_with(bundle))
            };
            let resolved = program
                .and_then(|p| std::fs::canonicalize(native(p)).ok())
                .and_then(|p| raw(&p));
            let label = service.label.raw().to_ascii_lowercase();
            let named = ids.iter().any(|id| {
                label
                    .strip_prefix(id.as_str())
                    .is_some_and(|rest| rest.is_empty() || rest.starts_with('.'))
            });
            if listed.contains(&index)
                || program.is_some_and(inside)
                || resolved.as_ref().is_some_and(inside)
                || named
            {
                count += 1;
            }
        }
        Some(count)
    }

    /// The application and the gate's evidence about it, observed now.
    struct Observed {
        app: UninstallApp,
        entry: Option<FilesystemEntry>,
        evidence: BundleEvidence,
    }

    impl Observed {
        fn binding(&self) -> Option<Binding<'_>> {
            Some(Binding {
                key: &self.app.key,
                bundle_id: &self.app.bundle_id,
                name: self.app.name.as_ref(),
                version: self.app.version.as_ref(),
                path: &self.app.path,
                identity: self.entry.as_ref()?.identity(),
                reclaim: self.app.reclaim,
                size_complete: self.app.size_complete,
            })
        }
    }

    fn raw(path: &Path) -> Option<RawPath> {
        crate::raw_path(path).ok()
    }

    /// Gathers the evidence about the bundle `key` names: its size first, so
    /// the checks the gate decides on are the freshest.
    fn observe(computer: &Computer<'_>, key_: &str) -> Result<Observed, UninstallError> {
        let apps = computer.software.applications().applications;
        let app = find(&apps, key_)?;
        let path = app
            .install_locations
            .first()
            .ok_or(UninstallError::Ambiguous)?
            .clone();
        let bundle = native(&path);
        let (reclaim, size_complete) = measure(&path, &computer.stores.paths());
        let entry = enumerator().stat(&path).ok();
        let owned_by_user = entry
            .as_ref()
            .and_then(|e| UnixOwnershipProbe.owned_by_current_user(&path, e.identity()));
        let services = computer.software.services();
        let linked_services = linked(&apps, app, &services);
        let helpers = helpers(&bundle);
        let running = computer.software.running(&path);
        let store_on_volume = computer.stores.locations.first().and_then(|(base, _)| {
            let store = std::fs::metadata(base).ok()?.dev();
            let bundle = std::fs::symlink_metadata(&bundle).ok()?.dev();
            Some(store == bundle)
        });
        let evidence = BundleEvidence {
            path: path.clone(),
            folders: computer.folders.iter().filter_map(|f| raw(f)).collect(),
            directory: entry.as_ref().map(|e| e.kind == EntryKind::Directory),
            owned_by_user,
            linked_services,
            helpers,
            running,
            store_on_volume,
        };
        let bundle_id = bundle_id(app).ok_or(UninstallError::UnknownApp)?.clone();
        Ok(Observed {
            app: UninstallApp {
                key: key(&bundle_id),
                bundle_id,
                name: app.name.clone(),
                version: app.version.clone(),
                path,
                reclaim,
                size_complete,
            },
            entry,
            evidence,
        })
    }

    pub(super) fn preview_with(
        computer: &Computer<'_>,
        db: &Path,
        key_: &str,
        clock: &dyn lumen_application::ports::Clock,
        ids: &dyn lumen_application::ports::IdGenerator,
    ) -> Result<UninstallPreview, UninstallError> {
        let observed = observe(computer, key_)?;
        if let Err(refusal) = gate(&observed.evidence) {
            tracing::info!(reasons = refusal.reasons().len(), "uninstall refused");
            return Ok(UninstallPreview {
                app: observed.app,
                ready: Err(refusal.reasons().to_vec()),
            });
        }
        let (Some(binding), Some(entry)) = (observed.binding(), observed.entry.as_ref()) else {
            // The gate passed only for an observed directory.
            return Err(UninstallError::UnknownApp);
        };
        let decided = decision(binding.hash());
        let item = PlannedItem {
            subject: Subject::File {
                identity: entry.identity().clone(),
            },
            path: observed.app.path.clone(),
            policy_version: decided.policy_version(),
            evidence_hash: decided.evidence_hash(),
            reclaim: observed.app.reclaim,
            chosen_by: Chooser::Person,
        };
        let plan = CleanupPlan::new(ids.plan_id(), clock.now(), vec![item]);
        IndexStore::new(open_index(db)?).save_plan(&plan, &[decided])?;
        tracing::info!("uninstall preview stored");
        Ok(UninstallPreview {
            app: observed.app,
            ready: Ok(Confirmable {
                plan: plan.id(),
                hash: plan.hash(),
                created_at: plan.created_at(),
            }),
        })
    }

    /// Decides the planned bundle again and, if it may still go, moves it.
    fn execute_item(
        computer: &Computer<'_>,
        executor: &dyn QuarantineExecutor,
        journal: &mut dyn QuarantineJournal,
        planned: &PlannedItem,
        stored: &PolicyDecision,
        manifest: impl Fn(&FilesystemEntry, &PreparedItem, OperationId) -> Option<Vec<u8>>,
        ids: &dyn lumen_application::ports::IdGenerator,
    ) -> Result<UninstallResult, UninstallError> {
        let Subject::File { identity } = &planned.subject else {
            return Ok(UninstallResult::Changed);
        };
        // The bundle at the planned path, found again by its bundle ID.
        let apps = computer.software.applications().applications;
        let Some(key_) = apps
            .iter()
            .filter(|a| a.install_locations.as_slice() == std::slice::from_ref(&planned.path))
            .find_map(bundle_id)
            .map(key)
        else {
            return Ok(UninstallResult::Changed);
        };
        let observed = match observe(computer, &key_) {
            Ok(observed) => observed,
            Err(UninstallError::UnknownApp | UninstallError::Ambiguous) => {
                return Ok(UninstallResult::Changed);
            }
            Err(error) => return Err(error),
        };
        if let Err(refusal) = gate(&observed.evidence) {
            return Ok(UninstallResult::Refused(refusal.reasons().to_vec()));
        }
        // The confirmed plan body and its stored decision must agree with what
        // is observed now; the body is what the confirmation hash covers.
        let unchanged = observed.app.path == planned.path
            && (!observed.app.size_complete || observed.app.reclaim == planned.reclaim)
            && observed.binding().is_some_and(|b| {
                let hash = b.hash();
                hash == planned.evidence_hash && hash == stored.evidence_hash()
            })
            && observed
                .entry
                .as_ref()
                .is_some_and(|e| e.identity() == identity);
        if !unchanged {
            return Ok(UninstallResult::Changed);
        }
        let folders: Vec<RawPath> = computer.folders.iter().filter_map(|f| raw(f)).collect();
        let Some((root, relative)) = split(&planned.path, &folders) else {
            return Ok(UninstallResult::Changed);
        };
        if relative.len() != 1 {
            return Ok(UninstallResult::Changed);
        }
        let Ok(prepared) = executor.prepare(&root, &relative) else {
            return Ok(UninstallResult::Changed);
        };
        // Observed after the executor recorded it: what it recorded is the
        // planned bundle, or the executor's own check refuses the move.
        let Some(entry) = enumerator()
            .stat(&planned.path)
            .ok()
            .filter(|e| e.identity() == identity && e.kind == EntryKind::Directory)
        else {
            return Ok(UninstallResult::Changed);
        };
        let operation = ids.operation_id();
        let Some(manifest) = manifest(&entry, &prepared, operation) else {
            return Ok(UninstallResult::Changed);
        };
        let ready = ConfirmedItem {
            operation,
            item: prepared,
            path: planned.path.clone(),
            identity: identity.clone(),
            manifest,
            // ADR-0062 (amended 2026-11-08): the gate just passed on fresh
            // evidence, so this is the admitted user-owned bundle and every
            // process's executable path was read with none inside it. A
            // partial answer then only leaves other users' open files unseen,
            // which the rename keeps working. `Yes` and `Unknown` still refuse.
            partial_in_use_accepted: true,
        };
        Ok(
            match quarantine_item(executor, journal, computer.in_use, &ready)? {
                Outcome::Retained => UninstallResult::Quarantined(operation),
                Outcome::Failed { reason, left } => UninstallResult::Failed { reason, left },
            },
        )
    }

    /// Records what the held app frees and that the person chose it, so it
    /// is never removed automatically (ADR-0051). It is already safe in the
    /// store, so a failure only warns: an item without a recorded chooser
    /// waits for the person too.
    fn record(ledger: &mut LedgerStore, operation: OperationId, reclaim: ByteCount) -> Vec<String> {
        let mut warnings = Vec::new();
        if let Err(error) = ledger.record_reclaim(operation, reclaim) {
            warnings.push(format!(
                "the size of the uninstalled app was not recorded: {error}"
            ));
        }
        if let Err(error) = ledger.record_chooser(operation, Chooser::Person) {
            warnings.push(format!(
                "who chose the uninstalled app was not recorded, so it waits for you to remove it: {error}"
            ));
        }
        warnings
    }

    pub(super) fn uninstall_with(
        computer: &Computer<'_>,
        db: &Path,
        plan: PlanId,
        confirm: &str,
        surface: ConfirmationSurface,
        clock: &dyn lumen_application::ports::Clock,
        ids: &dyn lumen_application::ports::IdGenerator,
    ) -> Result<UninstallReport, UninstallError> {
        (computer.privileged)()?;
        let now = clock.now();
        let mut index = IndexStore::new(open_index(db)?);
        let stored = checked_plan(&index, plan, confirm, now)?;
        let (planned, planned_decision) = match (stored.plan.items(), stored.decisions.as_slice()) {
            ([item], [decision])
                if is_uninstall(&stored.decisions)
                    && item.policy_version == GATE_VERSION
                    && item.chosen_by == Chooser::Person
                    && item.evidence_hash == decision.evidence_hash() =>
            {
                (item, decision)
            }
            _ => return Err(UninstallError::NotAnUninstall(plan)),
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
        let mut warnings = match computer.stores.locations.first() {
            Some((base, names)) => prepare_store(base, names)?,
            None => Vec::new(),
        };
        let executor = computer
            .stores
            .executor()
            .ok_or(QuarantineError::Location(LocationError::NoHome))?;
        let mut ledger = LedgerStore::new(open_ledger(&db.with_file_name("ledger.db"))?);
        let mut journal = LedgerJournal::new(&mut ledger, clock);
        let resolved = reconcile(&executor, &mut journal)?.len();
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
            computer,
            &executor,
            &mut journal,
            planned,
            planned_decision,
            manifest,
            ids,
        );
        let result = match result {
            Ok(result) => result,
            Err(error) => {
                // Nothing more runs under this plan; a journal failure after
                // the move is settled by reconciliation. The error that
                // stopped it is the one reported.
                if let Err(finish) = index.apply_plan_event(
                    plan,
                    PlanEvent::Finish {
                        all_succeeded: false,
                    },
                ) {
                    tracing::warn!(%finish, "the stopped uninstall plan was not marked finished");
                }
                return Err(error);
            }
        };
        let reclaim = planned.reclaim.reclaimable_now;
        if let UninstallResult::Quarantined(operation) = result {
            warnings.extend(record(&mut ledger, operation, reclaim));
        }
        let all_succeeded = matches!(result, UninstallResult::Quarantined(_));
        index.apply_plan_event(plan, PlanEvent::Finish { all_succeeded })?;
        tracing::info!(
            quarantined = all_succeeded,
            resolved,
            warnings = warnings.len(),
            "uninstall executed"
        );
        Ok(UninstallReport {
            plan,
            resolved,
            path: planned.path.clone(),
            reclaim,
            result,
            warnings,
        })
    }
}

#[cfg(target_os = "macos")]
mod mac {
    //! This Mac: its application folders, inventories and process table.

    use lumen_application::CancelToken;
    use lumen_application::ports::{InUseProbe, InventoryReport, InventorySource as _};
    use lumen_domain::RawPath;
    use lumen_platform_macos::{AppInventory, LaunchdInventory, ProcessProbe};

    use super::UninstallError;
    use super::unix::Computer;
    use crate::quarantine::QuarantineError;
    use crate::store::{LocationError, refuse_privileged, stores};

    pub(super) struct Software;

    impl super::unix::Software for Software {
        fn applications(&self) -> InventoryReport {
            AppInventory::new().collect(&CancelToken::new())
        }

        fn services(&self) -> InventoryReport {
            LaunchdInventory::new().collect(&CancelToken::new())
        }

        fn running(&self, bundle: &RawPath) -> Option<bool> {
            ProcessProbe.snapshot().running(bundle)
        }
    }

    pub(super) fn computer<'a>(
        software: &'a Software,
        in_use: &'a dyn InUseProbe,
    ) -> Result<Computer<'a>, UninstallError> {
        let home = std::env::home_dir().ok_or(QuarantineError::Location(LocationError::NoHome))?;
        Ok(Computer {
            folders: vec!["/Applications".into(), home.join("Applications")],
            stores: stores().map_err(QuarantineError::Location)?,
            software,
            in_use,
            privileged: refuse_privileged,
        })
    }
}

#[cfg(all(test, unix))]
#[allow(clippy::unwrap_used, clippy::panic, clippy::disallowed_methods)]
mod tests {
    //! Every test acts only inside its own temporary fixture: the
    //! applications folder, the quarantine store and the databases.

    use std::cell::{Cell, RefCell};
    use std::collections::BTreeSet;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicI64, Ordering};

    use lumen_application::finalize::{FinalizeOutcome, FinalizeReason, finalize_item};
    use lumen_application::ports::{InUse, InUseProbe, InventoryReport, PreparedItem};
    use lumen_application::restore::{RestoreOutcome, restore_item};
    use lumen_domain::{
        AppId, AppIdScheme, Application, Chooser, CleanupPlan, EvidenceHash, FileIdentity,
        FiredRule, JevEffect, PlanState, PlannedItem, PolicyDecision, PolicyStage, PolicyVersion,
        QuarantineEvent, QuarantineState, Risk, RiskLevel, RuleId, Service, ServiceKind,
        ServiceScope, SourceName, SourceStatus, Subject, Verdict,
    };
    use lumen_store_sqlite::{IndexStore, LedgerJournal, LedgerStore, open_index, open_ledger};
    use lumen_testkit::{Fixture, Node};

    use super::unix::{Computer, Software, preview_with, uninstall_with};
    use super::*;
    use crate::UuidV7Ids;
    use crate::store::Stores;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    const KEY: &str = "MacApp:dev.zed.Zed";

    struct Free;

    impl InUseProbe for Free {
        fn probe(&self, _: &RawPath, _: &FileIdentity) -> InUse {
            InUse::No
        }
    }

    /// The live in-use answer right before the move: `Some(false)` is `No`,
    /// `None` is `Partial`, `Some(true)` is `Yes`.
    struct Probe(std::sync::Mutex<Option<bool>>);

    impl InUseProbe for Probe {
        fn probe(&self, _: &RawPath, _: &FileIdentity) -> InUse {
            match *self
                .0
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
            {
                Some(false) => InUse::No,
                None => InUse::Partial { unseen: Some(3) },
                Some(true) => InUse::Yes(Vec::new()),
            }
        }
    }

    struct TestClock(AtomicI64);

    impl Clock for TestClock {
        fn now(&self) -> Timestamp {
            Timestamp::from_unix(self.0.load(Ordering::Relaxed), 0).unwrap()
        }
    }

    fn report(name: &str, status: SourceStatus) -> InventoryReport {
        InventoryReport {
            source: SourceName::new(name).unwrap(),
            status,
            applications: Vec::new(),
            services: Vec::new(),
            packages: Vec::new(),
        }
    }

    /// The inventories and process table of the fixture.
    struct Fake {
        apps: RefCell<Vec<Application>>,
        services: RefCell<InventoryReport>,
        running: Cell<Option<bool>>,
    }

    impl Software for Fake {
        fn applications(&self) -> InventoryReport {
            InventoryReport {
                applications: self.apps.borrow().clone(),
                ..report("test.apps", SourceStatus::Complete)
            }
        }

        fn services(&self) -> InventoryReport {
            self.services.borrow().clone()
        }

        fn running(&self, _: &RawPath) -> Option<bool> {
            self.running.get()
        }
    }

    fn raw(path: &Path) -> RawPath {
        crate::raw_path(path).unwrap()
    }

    fn app(path: &Path) -> Application {
        Application {
            ids: BTreeSet::from([AppId {
                scheme: AppIdScheme::BundleId,
                value: UntrustedText::new("dev.zed.Zed"),
            }]),
            name: Some(UntrustedText::new("Zed")),
            version: Some(UntrustedText::new("1.0")),
            vendor: None,
            signature: None,
            install_locations: vec![raw(path)],
            sources: BTreeSet::new(),
            uninstall: None,
        }
    }

    /// A fixture with the bundle, a home for the store and a data folder.
    struct Env {
        _fixture: Fixture,
        root: PathBuf,
        bundle: PathBuf,
        db: PathBuf,
        software: Fake,
        clock: TestClock,
        in_use: Probe,
    }

    impl Env {
        fn new(bundle: &str, extra: &[Node]) -> Result<Self, Box<dyn std::error::Error>> {
            let file = |path: String| Node::File {
                path,
                contents: b"app".to_vec(),
            };
            let mut nodes = vec![
                file(format!("{bundle}/Contents/Info.plist")),
                file(format!("{bundle}/Contents/MacOS/zed")),
                Node::Dir {
                    path: "home/Library/Application Support".into(),
                },
                Node::Dir {
                    path: "data".into(),
                },
            ];
            nodes.extend_from_slice(extra);
            let fixture = Fixture::build(&nodes)?;
            // Executors open no link, and temporary folders can sit below one.
            let root = std::fs::canonicalize(fixture.root())?;
            let bundle = root.join(bundle);
            Ok(Self {
                software: Fake {
                    apps: RefCell::new(vec![app(&bundle)]),
                    services: RefCell::new(report("test.launchd", SourceStatus::Complete)),
                    running: Cell::new(Some(false)),
                },
                db: root.join("data/index.db"),
                in_use: Probe(std::sync::Mutex::new(Some(false))),
                clock: TestClock(AtomicI64::new(1_800_000_000)),
                _fixture: fixture,
                root,
                bundle,
            })
        }

        fn eligible() -> Result<Self, Box<dyn std::error::Error>> {
            Self::new("Applications/Zed.app", &[])
        }

        fn stores(&self) -> Stores {
            Stores {
                locations: vec![(
                    self.root.join("home/Library/Application Support"),
                    vec![b"Lumen".to_vec(), b"Quarantine".to_vec()],
                )],
                volumes: None,
                single_user: false,
            }
        }

        fn computer(&self) -> Computer<'_> {
            Computer {
                folders: vec![self.root.join("Applications")],
                stores: self.stores(),
                software: &self.software,
                in_use: &self.in_use,
                privileged: || Ok(()),
            }
        }

        fn preview(&self) -> UninstallPreview {
            preview_with(&self.computer(), &self.db, KEY, &self.clock, &UuidV7Ids).unwrap()
        }

        fn ready(&self) -> Confirmable {
            self.preview().ready.unwrap()
        }

        fn run(&self, ready: &Confirmable) -> Result<UninstallReport, UninstallError> {
            self.confirm(ready.plan, &ready.hash.to_string())
        }

        fn confirm(&self, plan: PlanId, hash: &str) -> Result<UninstallReport, UninstallError> {
            uninstall_with(
                &self.computer(),
                &self.db,
                plan,
                hash,
                ConfirmationSurface::DesktopApp,
                &self.clock,
                &UuidV7Ids,
            )
        }

        fn installed(&self) -> bool {
            std::fs::symlink_metadata(&self.bundle).is_ok()
        }

        /// The names held in the store, without manifest sidecars.
        fn held(&self) -> Vec<String> {
            let store = self
                .root
                .join("home/Library/Application Support/Lumen/Quarantine");
            let Ok(entries) = std::fs::read_dir(store) else {
                return Vec::new();
            };
            let mut names: Vec<String> = entries
                .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
                .filter(|name| !name.contains('.'))
                .collect();
            names.sort();
            names
        }

        fn state(&self, plan: PlanId) -> PlanState {
            IndexStore::new(open_index(&self.db).unwrap())
                .plan(plan)
                .unwrap()
                .unwrap()
                .state
        }

        /// The executor's record of `operation`, from its journaled intent.
        fn record(&self, operation: OperationId) -> PreparedItem {
            let ledger =
                LedgerStore::new(open_ledger(&self.db.with_file_name("ledger.db")).unwrap());
            let (_, entries) = ledger
                .operations()
                .unwrap()
                .into_iter()
                .find(|(_, e)| e.first().is_some_and(|e| e.operation == operation))
                .unwrap();
            let intent = entries
                .iter()
                .find(|e| e.event == QuarantineEvent::Journal)
                .unwrap();
            PreparedItem(serde_json::to_vec(&intent.detail).unwrap())
        }
    }

    fn quarantined(report: &UninstallReport) -> OperationId {
        match report.result {
            UninstallResult::Quarantined(operation) => operation,
            ref other => panic!("not quarantined: {other:?}"),
        }
    }

    fn quarantine_error(result: Result<UninstallReport, UninstallError>) -> QuarantineError {
        match result {
            Err(UninstallError::Quarantine(error)) => *error,
            other => panic!("expected a refusal, got {other:?}"),
        }
    }

    #[test]
    fn an_eligible_app_moves_into_quarantine_and_is_restored_and_removed_for_good() -> TestResult {
        let env = Env::eligible()?;
        let preview = env.preview();
        assert_eq!(preview.app.key, KEY);
        assert_eq!(preview.app.path, raw(&env.bundle));
        assert!(preview.app.size_complete);
        assert!(preview.app.reclaim.logical.get() >= 6, "both files counted");
        let ready = preview.ready.clone().unwrap();
        assert!(matches!(env.state(ready.plan), PlanState::Proposed { .. }));

        let report = env.run(&ready)?;
        let operation = quarantined(&report);
        assert!(!env.installed(), "the bundle left the applications folder");
        assert_eq!(env.held(), [operation.to_string()]);
        assert_eq!(env.state(ready.plan), PlanState::Completed);
        assert_eq!(report.reclaim, preview.app.reclaim.reclaimable_now);

        // Listed in Quarantine like any item the person chose.
        let listed = crate::held::list(&env.db, &env.clock)?;
        let held = listed.iter().find(|h| h.operation == operation).unwrap();
        assert_eq!(held.chosen_by, Some(Chooser::Person));
        assert_eq!(held.original.as_deref(), Some(env.bundle.as_path()));
        assert_eq!(held.reclaim, Some(report.reclaim));

        // A confirmation is used once.
        assert!(matches!(
            quarantine_error(env.run(&ready)),
            QuarantineError::AlreadyConfirmed(_)
        ));

        // Restored with one action...
        let executor = env.stores().executor().unwrap();
        let mut ledger = LedgerStore::new(open_ledger(&env.db.with_file_name("ledger.db"))?);
        let restored = restore_item(
            &executor,
            &mut LedgerJournal::new(&mut ledger, &env.clock),
            operation,
            QuarantineState::Retained,
            &env.record(operation),
        )?;
        assert!(matches!(
            restored,
            RestoreOutcome::Restored { verified: true, .. }
        ));
        assert!(env.installed());
        assert_eq!(env.held(), Vec::<String>::new());

        // ...or, uninstalled again, removed for good after the tree is checked.
        let again = quarantined(&env.run(&env.ready())?);
        let finalized = finalize_item(
            &executor,
            &mut LedgerJournal::new(&mut ledger, &env.clock),
            again,
            QuarantineState::Retained,
            &env.record(again),
            FinalizeReason::UserRequest,
            &Free,
        )?;
        assert_eq!(finalized, FinalizeOutcome::Finalized);
        assert!(!env.installed());
        assert_eq!(env.held(), Vec::<String>::new());
        Ok(())
    }

    #[test]
    fn each_ineligible_app_is_refused_and_nothing_is_stored_or_moved() -> TestResult {
        let dir = |path: &str| Node::Dir { path: path.into() };
        let helper = Env::new(
            "Applications/Zed.app",
            &[dir("Applications/Zed.app/Contents/Library/LoginItems")],
        )?;
        let nested = Env::new("Applications/Utilities/Zed.app", &[])?;
        let linked = Env::eligible()?;
        linked
            .software
            .services
            .borrow_mut()
            .services
            .push(Service {
                kind: ServiceKind::LaunchAgent,
                scope: ServiceScope::User,
                label: UntrustedText::new("dev.zed.Zed.helper"),
                definition: None,
                program: Some(raw(&linked.bundle.join("Contents/MacOS/zed"))),
                enabled: None,
                signature: None,
            });
        let unread = Env::eligible()?;
        *unread.software.services.borrow_mut() = report("test.launchd", SourceStatus::Partial);
        let running = Env::eligible()?;
        running.software.running.set(Some(true));
        let unknown = Env::eligible()?;
        unknown.software.running.set(None);
        let link = Env::new(
            "elsewhere/Zed.app",
            &[
                dir("Applications"),
                Node::Symlink {
                    path: "Applications/Zed.app".into(),
                    target: "../elsewhere/Zed.app".into(),
                },
            ],
        )?;
        let link_path = link.root.join("Applications/Zed.app");
        *link.software.apps.borrow_mut() = vec![app(&link_path)];

        for (env, reason) in [
            (&helper, Ineligible::HasHelpers),
            (&nested, Ineligible::NotInApplications),
            (&linked, Ineligible::ServiceLinked),
            (&unread, Ineligible::ServicesUnknown),
            (&running, Ineligible::Running),
            (&unknown, Ineligible::RunningUnknown),
        ] {
            let preview = env.preview();
            assert_eq!(preview.ready, Err(vec![reason]), "{reason:?}");
            assert!(env.installed(), "{reason:?}: not moved");
            assert_eq!(env.held(), Vec::<String>::new(), "{reason:?}");
        }
        // A link in place of the bundle is refused, and so is whatever it
        // points to.
        let reasons = link.preview().ready.unwrap_err();
        assert!(reasons.contains(&Ineligible::NotABundle), "{reasons:?}");
        assert!(
            std::fs::symlink_metadata(&link_path)?
                .file_type()
                .is_symlink()
        );
        assert!(link.installed());

        let missing = Env::eligible()?;
        missing.software.apps.borrow_mut().clear();
        assert!(matches!(
            preview_with(
                &missing.computer(),
                &missing.db,
                KEY,
                &missing.clock,
                &UuidV7Ids
            ),
            Err(UninstallError::UnknownApp)
        ));
        let twice = Env::eligible()?;
        let copy = app(&twice.root.join("home/Applications/Zed.app"));
        twice.software.apps.borrow_mut().push(copy);
        assert!(matches!(
            preview_with(&twice.computer(), &twice.db, KEY, &twice.clock, &UuidV7Ids),
            Err(UninstallError::Ambiguous)
        ));
        Ok(())
    }

    #[test]
    fn a_background_item_that_may_run_the_app_by_any_route_refuses_it() -> TestResult {
        let agent = |env: &Env, label: &str, program: PathBuf| {
            env.software.services.borrow_mut().services.push(Service {
                kind: ServiceKind::LaunchAgent,
                scope: ServiceScope::User,
                label: UntrustedText::new(label),
                definition: None,
                program: Some(raw(&program)),
                enabled: None,
                signature: None,
            });
        };
        // Another spelling of the bundle's path, which a case-insensitive
        // volume runs.
        let shouted = Env::eligible()?;
        let program = shouted.root.join("applications/zed.APP/Contents/MacOS/cli");
        agent(&shouted, "com.example.sync", program);
        // A link elsewhere to a program inside the bundle.
        let linked = Env::new(
            "Applications/Zed.app",
            &[
                Node::Dir { path: "bin".into() },
                Node::Symlink {
                    path: "bin/zed".into(),
                    target: "../Applications/Zed.app/Contents/MacOS/zed".into(),
                },
            ],
        )?;
        let program = linked.root.join("bin/zed");
        agent(&linked, "com.example.cli", program);
        // The bundle ID in another case.
        let named = Env::eligible()?;
        agent(&named, "DEV.ZED.ZED.sync", PathBuf::from("/usr/bin/true"));
        // `open -a`: which app it starts is in arguments Lumen does not keep.
        let launcher = Env::eligible()?;
        agent(
            &launcher,
            "com.example.opener",
            PathBuf::from("/usr/bin/open"),
        );
        for (env, reason) in [
            (&shouted, Ineligible::ServiceLinked),
            (&linked, Ineligible::ServiceLinked),
            (&named, Ineligible::ServiceLinked),
            (&launcher, Ineligible::ServicesUnknown),
        ] {
            assert_eq!(env.preview().ready, Err(vec![reason]), "{reason:?}");
            assert!(env.installed(), "{reason:?}: not moved");
        }
        // An item that runs something else does not refuse it.
        let unrelated = Env::eligible()?;
        agent(
            &unrelated,
            "com.example.other",
            PathBuf::from("/usr/bin/true"),
        );
        assert!(unrelated.preview().ready.is_ok());
        Ok(())
    }

    #[test]
    fn an_app_that_changed_or_started_after_its_preview_is_not_moved() -> TestResult {
        let grown = Env::eligible()?;
        let ready = grown.ready();
        std::fs::write(grown.bundle.join("Contents/MacOS/update"), b"new code")?;
        let report = grown.run(&ready)?;
        assert_eq!(report.result, UninstallResult::Changed);
        assert!(grown.installed());
        assert_eq!(grown.held(), Vec::<String>::new());
        assert!(matches!(
            grown.state(ready.plan),
            PlanState::PartiallyCompleted
        ));

        let started = Env::eligible()?;
        let ready = started.ready();
        started.software.running.set(Some(true));
        let report = started.run(&ready)?;
        assert_eq!(
            report.result,
            UninstallResult::Refused(vec![Ineligible::Running])
        );
        assert!(started.installed());

        let gone = Env::eligible()?;
        let ready = gone.ready();
        gone.software.apps.borrow_mut().clear();
        assert_eq!(gone.run(&ready)?.result, UninstallResult::Changed);
        assert!(gone.installed());
        Ok(())
    }

    #[test]
    fn a_partial_in_use_answer_moves_an_app_nothing_runs_from_and_a_user_keeps_it() -> TestResult {
        // Other users' open files cannot be read: ADR-0062 accepts that once
        // the gate found no process running from the bundle.
        let partial = Env::eligible()?;
        let ready = partial.ready();
        *partial.in_use.0.lock().unwrap() = None;
        let report = partial.run(&ready)?;
        assert!(matches!(report.result, UninstallResult::Quarantined(_)));
        assert!(!partial.installed());

        // A process seen using the bundle still keeps it where it is.
        let used = Env::eligible()?;
        let ready = used.ready();
        *used.in_use.0.lock().unwrap() = Some(true);
        let report = used.run(&ready)?;
        assert!(
            matches!(
                report.result,
                UninstallResult::Failed {
                    reason: QuarantineFailure::InUse,
                    left: ItemLocation::Source,
                }
            ),
            "{:?}",
            report.result
        );
        assert!(used.installed());

        // Unknown whether it runs: refused before the probe is asked.
        let unknown = Env::eligible()?;
        let ready = unknown.ready();
        unknown.software.running.set(None);
        *unknown.in_use.0.lock().unwrap() = None;
        let report = unknown.run(&ready)?;
        assert_eq!(
            report.result,
            UninstallResult::Refused(vec![Ineligible::RunningUnknown])
        );
        assert!(unknown.installed());
        Ok(())
    }

    #[test]
    fn a_wrong_expired_or_privileged_confirmation_moves_nothing() -> TestResult {
        let env = Env::eligible()?;
        let ready = env.ready();
        let hash = ready.hash.to_string();
        for wrong in [&hash[..hash.len() - 1], "", "b3:00"] {
            assert!(matches!(
                quarantine_error(env.confirm(ready.plan, wrong)),
                QuarantineError::Mismatch(_)
            ));
        }
        let unknown: PlanId = "0190f0d2-6c1e-7a3b-8c4d-5e6f708192a3".parse()?;
        assert!(matches!(
            quarantine_error(env.confirm(unknown, &hash)),
            QuarantineError::UnknownPlan(_)
        ));

        let privileged = Computer {
            privileged: || Err(PrivilegeError::Root),
            ..env.computer()
        };
        let refused = uninstall_with(
            &privileged,
            &env.db,
            ready.plan,
            &hash,
            ConfirmationSurface::DesktopApp,
            &env.clock,
            &UuidV7Ids,
        );
        assert!(matches!(
            quarantine_error(refused),
            QuarantineError::Privileged(PrivilegeError::Root)
        ));
        assert!(env.installed());
        assert!(matches!(env.state(ready.plan), PlanState::Proposed { .. }));

        env.clock
            .0
            .fetch_add(crate::quarantine::MAX_PLAN_AGE_SECS + 1, Ordering::Relaxed);
        assert!(matches!(
            quarantine_error(env.run(&ready)),
            QuarantineError::TooOld(_)
        ));
        assert!(env.installed());
        assert_eq!(env.held(), Vec::<String>::new());
        Ok(())
    }

    #[test]
    fn cleanup_and_uninstall_plans_refuse_each_other() -> TestResult {
        let env = Env::eligible()?;
        let ready = env.ready();
        assert!(matches!(
            crate::quarantine::quarantine(
                &env.db,
                ready.plan,
                &ready.hash.to_string(),
                ConfirmationSurface::DesktopApp,
                &env.clock,
                &UuidV7Ids,
            ),
            Err(QuarantineError::NotACleanup(_))
        ));
        assert!(matches!(env.state(ready.plan), PlanState::Proposed { .. }));

        let entry = lumen_application::ports::DirEnumerator::stat(
            &crate::scan::enumerator(),
            &raw(&env.bundle),
        )?;
        let cleanup = CleanupPlan::new(
            UuidV7Ids.plan_id(),
            env.clock.now(),
            vec![PlannedItem {
                subject: Subject::File {
                    identity: entry.identity().clone(),
                },
                path: raw(&env.bundle),
                policy_version: PolicyVersion::new(0, 10, 0),
                evidence_hash: EvidenceHash::from_bytes([1; 32]),
                reclaim: ReclaimEstimate::default(),
                chosen_by: Chooser::Policy,
            }],
        );
        let decision = PolicyDecision::new(
            Verdict::Quarantine,
            PolicyVersion::new(0, 10, 0),
            EvidenceHash::from_bytes([1; 32]),
            [FiredRule {
                rule: RuleId::new("classify.app-cache")?,
                stage: PolicyStage::Classification,
                evidence: BTreeSet::new(),
            }],
            Risk {
                level: RiskLevel::Low,
                factors: BTreeSet::new(),
            },
            JevEffect::None,
        )?;
        IndexStore::new(open_index(&env.db)?).save_plan(&cleanup, &[decision])?;
        assert!(matches!(
            env.confirm(cleanup.id(), &cleanup.hash().to_string()),
            Err(UninstallError::NotAnUninstall(_))
        ));
        assert!(env.installed());
        assert_eq!(env.held(), Vec::<String>::new());
        Ok(())
    }
}
