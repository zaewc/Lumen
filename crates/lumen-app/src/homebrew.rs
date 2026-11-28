//! Removing a Homebrew formula or cask through Homebrew (ADR-0060, roadmap
//! 16.7, #809): the preview.
//!
//! [`preview`] finds the one package the software list names by its key
//! (`Homebrew:<name>` or `HomebrewCask:<token>`), gathers fresh read-only
//! evidence about it, and runs the gate
//! ([`lumen_application::homebrew_removal`]):
//!
//! - the Homebrew inventory, read whole, lists it once, and its records say
//!   no listed package needs it;
//! - Homebrew's own records say its uninstall runs with the person's rights
//!   ([`lumen_platform_macos::HomebrewInventory::removal_rights`]);
//! - no app a cask placed is security software (the Endpoint Security
//!   entitlement) or may be managed by an organization (MDM enrollment or
//!   managed preferences), as an app's uninstall checks (ADR-0062, #973); a
//!   formula places no app, so only enrollment counts;
//! - nothing runs from its folder or an app it placed.
//!
//! If it passes, the preview is stored as a one-item plan chosen by the
//! person, whose hash the person confirms (ADR-0034); the page sends only its
//! ID (ADR-0023). The plan names the package's folder in Homebrew's prefix by
//! identity, and its decision binds the package, the folder, `brew` in that
//! same prefix by absolute path, and the exact command (`brew uninstall
//! --formula|--cask <name>`, never `--zap`). Its rule keeps it out of every
//! other action: a clean-up, an app's uninstall and a copy's move each refuse
//! it, so its folder is never moved into quarantine (ADR-0062 rule 5).
//!
//! Making a preview runs nothing, and no file contents are read beyond
//! Homebrew's own JSON records.
//!
//! [`remove`] runs exactly that preview, once (ADR-0060 rules 3 and 7, as
//! ADR-0049 runs a tool): it refuses a privileged process and a wrong,
//! expired, reused or foreign confirmation, then gathers the evidence again,
//! runs the gate again and refuses a package that changed since its preview.
//! Only then does it run the bound command: `brew` by absolute path in
//! Homebrew's own prefix, never found on a `PATH`, with no shell, input
//! closed, its own process group, in the home folder, for at most
//! [`crate::tools::TIME_LIMIT`], without Homebrew's auto-update. It is never
//! retried. The package counts as removed only when a fresh Homebrew list,
//! read whole, no longer lists it; the exit status and the end of the
//! output are reported as they are. Lumen cannot undo it.

use std::path::Path;

use lumen_application::homebrew_removal::Blocked;
use lumen_application::ports::{Clock, IdGenerator};
use lumen_domain::{
    ConfirmationSurface, PackageId, PlanHash, PlanId, RawPath, Timestamp, UntrustedText,
};

use crate::quarantine::QuarantineError;
use crate::tools::ToolError;

/// Why no preview was made, or a preview was not run. Nothing was stored or
/// run in any of these cases, except [`HomebrewError::Run`].
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum HomebrewError {
    /// Homebrew removal is available only on macOS.
    #[error("removing a Homebrew package is available only on macOS")]
    Unsupported,
    /// The Homebrew list has no package with that key.
    #[error("Homebrew lists no package with that key; list the software again")]
    UnknownPackage,
    /// The database failed.
    #[error(transparent)]
    Store(#[from] lumen_store_sqlite::StoreError),
    /// The plan is not a Homebrew removal preview.
    #[error("plan {0} is not a Homebrew removal preview; nothing was run")]
    NotARemoval(PlanId),
    /// The preview may not run: a privileged process, or an unknown, wrong,
    /// expired or already confirmed preview.
    #[error(transparent)]
    Plan(Box<QuarantineError>),
    /// `brew` could not be started or watched.
    #[error(transparent)]
    Run(ToolError),
}

impl From<QuarantineError> for HomebrewError {
    fn from(error: QuarantineError) -> Self {
        Self::Plan(Box::new(error))
    }
}

/// The package a preview is about, as observed now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HomebrewPackage {
    /// The software list's key.
    pub key: String,
    /// The package, by manager and name.
    pub package: PackageId,
    /// Its version, as listed.
    pub version: Option<UntrustedText>,
    /// Its folder in Homebrew's prefix, if Homebrew's list recorded it.
    pub folder: Option<RawPath>,
    /// `brew` in the same prefix, by absolute path, if it is there.
    pub program: Option<RawPath>,
    /// The apps a cask placed, which go with it; empty for a formula.
    pub apps: Vec<RawPath>,
}

/// A stored preview the person can confirm.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Confirmable {
    /// The preview's ID (a plan ID).
    pub plan: PlanId,
    /// Its hash: the confirmation.
    pub hash: PlanHash,
    /// When it was made; it expires 24 hours later.
    pub created_at: Timestamp,
    /// The exact command: the program by absolute path, then its arguments.
    pub command: Vec<String>,
}

/// What removing a Homebrew package would do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HomebrewPreview {
    /// The package.
    pub package: HomebrewPackage,
    /// The preview to confirm, or every reason it is not removed (nothing was
    /// stored then).
    pub ready: Result<Confirmable, Vec<Blocked>>,
}

/// How `brew` ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ran {
    /// It exited with status 0.
    pub success: bool,
    /// Its exit code, if it exited on its own.
    pub exit_code: Option<i32>,
    /// It ran past the time limit and was stopped.
    pub timed_out: bool,
    /// The end of what it printed, at most
    /// [`crate::tools::OUTPUT_LIMIT`] bytes.
    pub output: String,
}

/// What became of the package.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemovalResult {
    /// A fresh Homebrew list, read whole, no longer lists it.
    Removed,
    /// A fresh Homebrew list still lists it.
    StillListed,
    /// A fresh Homebrew list could not be read whole, so whether it is gone
    /// is not known.
    Unverified,
    /// Checked again right before running, the gate refused it; nothing ran.
    Refused(Vec<Blocked>),
    /// It is not what its preview showed; nothing ran. Preview it again.
    Changed,
}

/// What running a preview did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemovalReport {
    /// The preview run.
    pub plan: PlanId,
    /// The package, as observed right before running, if it was found.
    pub package: Option<HomebrewPackage>,
    /// The command run, if it ran.
    pub command: Option<Vec<String>>,
    /// How it ended, if it ran.
    pub ran: Option<Ran>,
    /// What became of the package.
    pub result: RemovalResult,
}

/// What the person confirms right before a preview runs: the package and the
/// exact command, observed now and checked against what the preview bound.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Question {
    /// The package.
    pub package: HomebrewPackage,
    /// The exact command.
    pub command: Vec<String>,
}

/// The question for running preview `plan`, after checking `confirm` (its
/// full hash), its age and that it still binds what is observed now; `None`
/// if the gate refuses it now or it changed. Reads only.
///
/// # Errors
///
/// As [`remove`] before it runs anything.
#[tracing::instrument(name = "homebrew_question", skip_all)]
pub fn question(
    db: &Path,
    plan: PlanId,
    confirm: &str,
    clock: &dyn Clock,
) -> Result<Option<Question>, HomebrewError> {
    #[cfg(target_os = "macos")]
    {
        crate::store::refuse_privileged().map_err(QuarantineError::from)?;
        unix::question_with(&mac::Mac::new(), db, plan, confirm, clock)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (db, plan, confirm, clock);
        Err(HomebrewError::Unsupported)
    }
}

/// Runs the Homebrew removal preview `plan` from `db`, confirmed on `surface`
/// by `confirm` (its full hash), once.
///
/// # Errors
///
/// A [`HomebrewError`] if the preview may not run, the database fails or
/// `brew` cannot be started. A refusal at the gate, a changed package, a
/// failed or stopped `brew` and a package still listed are not errors; they
/// are the report's [`RemovalReport::result`] and [`RemovalReport::ran`].
#[tracing::instrument(name = "homebrew_remove", skip_all)]
pub fn remove(
    db: &Path,
    plan: PlanId,
    confirm: &str,
    surface: ConfirmationSurface,
    clock: &dyn Clock,
) -> Result<RemovalReport, HomebrewError> {
    #[cfg(target_os = "macos")]
    {
        crate::store::refuse_privileged().map_err(QuarantineError::from)?;
        unix::remove_with(&mac::Mac::new(), db, plan, confirm, surface, clock)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (db, plan, confirm, surface, clock);
        Err(HomebrewError::Unsupported)
    }
}

/// Previews removing the Homebrew package listed under `key`, and stores the
/// preview if the gate passes.
///
/// # Errors
///
/// [`HomebrewError::Unsupported`] off macOS,
/// [`HomebrewError::UnknownPackage`] if Homebrew lists no package under
/// `key`, or a database error.
#[tracing::instrument(name = "homebrew_preview", skip_all)]
pub fn preview(
    db: &Path,
    key: &str,
    clock: &dyn Clock,
    ids: &dyn IdGenerator,
) -> Result<HomebrewPreview, HomebrewError> {
    #[cfg(target_os = "macos")]
    {
        unix::preview_with(&mac::Mac::new(), db, key, clock, ids)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (db, key, clock, ids);
        Err(HomebrewError::Unsupported)
    }
}

#[cfg(unix)]
#[cfg_attr(
    not(target_os = "macos"),
    allow(dead_code, reason = "only macOS uses it outside tests")
)]
mod unix {
    //! The parts that read Unix metadata and store the preview.

    use std::os::unix::ffi::OsStrExt as _;
    use std::path::{Path, PathBuf};

    use lumen_application::homebrew_removal::{
        Binding, Blocked, HomebrewCommand, HomebrewEvidence, PROGRAM, decision, homebrew_removal,
    };
    use lumen_application::ports::{Clock, DirEnumerator as _, IdGenerator, InventoryReport};
    use lumen_application::removal_facts::inventory_facts;
    use lumen_application::uninstall::evidence::bundle_id;
    use lumen_domain::{
        Chooser, CleanupPlan, EntryKind, FileIdentity, Package, PackageId, PackageManager,
        PackageRelationKind, PlannedItem, RawPath, ReclaimEstimate, SourceStatus, Subject,
        UntrustedText,
    };
    use lumen_store_sqlite::{IndexStore, open_index};

    use lumen_application::homebrew_removal::{GATE_VERSION, is_removal};
    use lumen_domain::{Confirmation, ConfirmationSurface, PlanEvent, PlanHash, PlanId};
    use lumen_store_sqlite::StoreError;

    use super::{
        Confirmable, HomebrewError, HomebrewPackage, HomebrewPreview, Question, Ran, RemovalReport,
        RemovalResult,
    };
    use crate::quarantine::{QuarantineError, checked_plan};
    use crate::scan::enumerator;
    use crate::tools::ToolError;

    /// What a removal looks at: this Mac, or a fixture in tests.
    pub(super) trait Homebrew {
        /// Homebrew's formulae and casks.
        fn packages(&self) -> InventoryReport;
        /// The applications, to name a cask's apps by bundle ID.
        fn applications(&self) -> InventoryReport;
        /// Whether Homebrew's uninstall of `package` runs with this user's
        /// rights.
        fn rights(&self, package: &PackageId) -> Option<bool>;
        /// Whether a process runs a program inside any of `folders`.
        fn running(&self, folders: &[RawPath]) -> Option<bool>;
        /// Whether the app at `bundle` is security software.
        fn security_software(&self, bundle: &RawPath) -> Option<bool>;
        /// Whether this Mac is enrolled in MDM.
        fn enrolled(&self) -> Option<bool>;
        /// Whether an organization may manage the app `bundle_id`.
        fn managed(&self, bundle_id: &UntrustedText) -> Option<bool>;
        /// Runs `program` with `args`, as ADR-0049 runs a tool. Tests use a
        /// fake; nothing else runs `brew`.
        fn run(&self, program: &RawPath, args: &[String]) -> Result<Ran, ToolError>;
    }

    /// The software list's key of a Homebrew package, as `lumen-api` lists
    /// it; `None` for another manager.
    pub(super) fn key(package: &Package) -> Option<String> {
        let origin = match package.manager {
            PackageManager::Homebrew => "Homebrew",
            PackageManager::HomebrewCask => "HomebrewCask",
            _ => return None,
        };
        Some(format!("{origin}:{}", package.id.display()))
    }

    pub(super) fn native(path: &RawPath) -> PathBuf {
        PathBuf::from(std::ffi::OsStr::from_bytes(path.as_bytes()))
    }

    /// Folds answers: any yes is yes, else any unknown is unknown.
    fn any(answers: impl IntoIterator<Item = Option<bool>>) -> Option<bool> {
        let mut result = Some(false);
        for answer in answers {
            match answer {
                Some(true) => return Some(true),
                Some(false) => {}
                None => result = None,
            }
        }
        result
    }

    /// The package, the gate's answer, and what the answer is bound to.
    pub(super) struct Observed {
        pub(super) package: HomebrewPackage,
        pub(super) identity: Option<FileIdentity>,
        pub(super) ready: Result<HomebrewCommand, Vec<Blocked>>,
    }

    impl Observed {
        /// The facts a stored preview binds, once the gate passed.
        pub(super) fn binding(&self) -> Option<Binding<'_>> {
            Some(Binding {
                key: &self.package.key,
                package: &self.package.package,
                version: self.package.version.as_ref(),
                folder: self.package.folder.as_ref()?,
                identity: self.identity.as_ref()?,
                program: self.package.program.as_ref()?,
                command: self.ready.as_ref().ok()?,
                apps: &self.package.apps,
            })
        }

        /// The exact command shown and run: the program, then its arguments.
        pub(super) fn command(&self) -> Option<Vec<String>> {
            let program = self.package.program.as_ref()?.display();
            let args = &self.ready.as_ref().ok()?.args;
            Some(
                std::iter::once(program)
                    .chain(args.iter().cloned())
                    .collect(),
            )
        }
    }

    /// `brew` in the prefix that holds `folder` (`<prefix>/Cellar/<name>`),
    /// if it is a program this user may run there.
    fn program(folder: &RawPath) -> Option<RawPath> {
        let prefix = native(folder).parent()?.parent()?.to_path_buf();
        let found = crate::tools::find_in(PROGRAM, &[prefix.join("bin")])?;
        crate::raw_path(&found).ok()
    }

    /// Gathers the evidence about the package `key_` names and runs the gate.
    pub(super) fn observe(brew: &dyn Homebrew, key_: &str) -> Result<Observed, HomebrewError> {
        let report = brew.packages();
        let matching: Vec<usize> = (0..report.packages.len())
            .filter(|&i| key(&report.packages[i]).as_deref() == Some(key_))
            .collect();
        let &first = matching.first().ok_or(HomebrewError::UnknownPackage)?;
        let listed = &report.packages[first];
        let package = listed.package_id();
        // Only Homebrew's records name Homebrew packages, so its own report
        // holds every dependent.
        let mut facts = inventory_facts(std::slice::from_ref(&report))
            .packages
            .get(first)
            .copied();
        if let Some(facts) = &mut facts {
            facts.protected = listed.required;
        }
        let folder = listed.install_root.clone();
        let identity = folder.as_ref().and_then(|folder| {
            enumerator()
                .stat(folder)
                .ok()
                .filter(|entry| entry.kind == EntryKind::Directory)
                .map(|entry| entry.identity().clone())
        });
        let program = folder.as_ref().and_then(program);
        // The apps a cask's records say it placed; `None` if not read.
        let apps: Option<Vec<RawPath>> = match package.manager {
            PackageManager::HomebrewCask => listed.relations.as_ref().map(|relations| {
                relations
                    .iter()
                    .filter_map(|relation| match &relation.kind {
                        PackageRelationKind::Installed { location } => Some(location.clone()),
                        _ => None,
                    })
                    .collect()
            }),
            _ => Some(Vec::new()),
        };
        let found = apps.as_deref().unwrap_or_default();
        // Read only for a cask's apps, to name them by bundle ID.
        let applications = if found.is_empty() {
            Vec::new()
        } else {
            brew.applications().applications
        };
        // A formula places no app bundle, which is where the Endpoint
        // Security entitlement is read (#937), so only a cask's apps count.
        let security_software = apps
            .as_ref()
            .and_then(|apps| any(apps.iter().map(|app| brew.security_software(app))));
        let managed = apps.as_ref().and_then(|apps| {
            let each = apps.iter().map(|app| {
                let mut ids = applications
                    .iter()
                    .filter(|a| a.install_locations.as_slice() == std::slice::from_ref(app))
                    .filter_map(bundle_id);
                match (ids.next(), ids.next()) {
                    (Some(id), None) => brew.managed(id),
                    _ => None,
                }
            });
            any(std::iter::once(brew.enrolled()).chain(each))
        });
        let running = match (&folder, &apps) {
            (Some(folder), Some(apps)) => {
                let folders: Vec<RawPath> = std::iter::once(folder.clone())
                    .chain(apps.iter().cloned())
                    .collect();
                brew.running(&folders)
            }
            _ => None,
        };
        // The folder and `brew` beside it must be there: without them the
        // command could not be bound to this prefix.
        let user_rights_only = if identity.is_some() && program.is_some() {
            brew.rights(&package)
        } else {
            None
        };
        let ready = homebrew_removal(&HomebrewEvidence {
            package: &package,
            listed: matching.len(),
            inventory_complete: report.status == SourceStatus::Complete,
            facts: facts.as_ref(),
            user_rights_only,
            security_software,
            managed,
            running,
        });
        Ok(Observed {
            package: HomebrewPackage {
                key: key_.to_owned(),
                version: listed.version.clone(),
                package,
                folder,
                program,
                apps: found.to_vec(),
            },
            identity,
            ready,
        })
    }

    pub(super) fn preview_with(
        brew: &dyn Homebrew,
        db: &Path,
        key_: &str,
        clock: &dyn Clock,
        ids: &dyn IdGenerator,
    ) -> Result<HomebrewPreview, HomebrewError> {
        let observed = observe(brew, key_)?;
        let (Some(binding), Some(command)) = (observed.binding(), observed.command()) else {
            // Without its folder or `brew` beside it, the rights are not
            // asked and the gate refuses; a preview is never stored unbound.
            let reasons = observed
                .ready
                .clone()
                .err()
                .unwrap_or_else(|| vec![Blocked::AdministratorUnknown]);
            tracing::info!(reasons = reasons.len(), "Homebrew removal refused");
            return Ok(HomebrewPreview {
                package: observed.package,
                ready: Err(reasons),
            });
        };
        let decided = decision(binding.hash());
        let item = PlannedItem {
            subject: Subject::File {
                identity: binding.identity.clone(),
            },
            path: binding.folder.clone(),
            policy_version: decided.policy_version(),
            evidence_hash: decided.evidence_hash(),
            reclaim: ReclaimEstimate::default(),
            chosen_by: Chooser::Person,
            scratch_change: None,
        };
        let plan = CleanupPlan::new(ids.plan_id(), clock.now(), vec![item]);
        IndexStore::new(open_index(db)?).save_plan(&plan, &[decided])?;
        tracing::info!("Homebrew removal preview stored");
        Ok(HomebrewPreview {
            package: observed.package,
            ready: Ok(Confirmable {
                plan: plan.id(),
                hash: plan.hash(),
                created_at: plan.created_at(),
                command,
            }),
        })
    }

    /// The one item of stored preview `plan` and the preview's hash, after
    /// checking `confirm`, its age and that it is a Homebrew removal preview.
    fn stored(
        index: &IndexStore,
        plan: PlanId,
        confirm: &str,
        now: lumen_domain::Timestamp,
    ) -> Result<(PlannedItem, PlanHash), HomebrewError> {
        let stored = checked_plan(index, plan, confirm, now)?;
        match (stored.plan.items(), stored.decisions.as_slice()) {
            ([item], [decision])
                if is_removal(&stored.decisions)
                    && item.policy_version == GATE_VERSION
                    && item.chosen_by == Chooser::Person
                    && item.evidence_hash == decision.evidence_hash() =>
            {
                Ok((item.clone(), stored.plan.hash()))
            }
            _ => Err(HomebrewError::NotARemoval(plan)),
        }
    }

    /// What is observed now about the package `planned` names by its folder.
    enum Check {
        /// The gate passes and the preview still binds it.
        Ready(Box<Observed>),
        /// The gate refuses it now.
        Refused(HomebrewPackage, Vec<Blocked>),
        /// It is not what the preview showed, or not found.
        Changed(Option<HomebrewPackage>),
    }

    fn check(brew: &dyn Homebrew, planned: &PlannedItem) -> Result<Check, HomebrewError> {
        let Subject::File { identity } = &planned.subject else {
            return Ok(Check::Changed(None));
        };
        let report = brew.packages();
        let mut found = report
            .packages
            .iter()
            .filter(|p| p.install_root.as_ref() == Some(&planned.path))
            .filter_map(key);
        let (Some(listed), None) = (found.next(), found.next()) else {
            return Ok(Check::Changed(None));
        };
        let observed = match observe(brew, &listed) {
            Ok(observed) => observed,
            Err(HomebrewError::UnknownPackage) => return Ok(Check::Changed(None)),
            Err(error) => return Err(error),
        };
        if let Err(reasons) = &observed.ready {
            return Ok(Check::Refused(observed.package, reasons.clone()));
        }
        // The binding covers the folder, `brew`, the command and the apps.
        let unchanged = observed.identity.as_ref() == Some(identity)
            && observed
                .binding()
                .is_some_and(|b| b.hash() == planned.evidence_hash);
        Ok(if unchanged {
            Check::Ready(Box::new(observed))
        } else {
            Check::Changed(Some(observed.package))
        })
    }

    pub(super) fn question_with(
        brew: &dyn Homebrew,
        db: &Path,
        plan: PlanId,
        confirm: &str,
        clock: &dyn Clock,
    ) -> Result<Option<Question>, HomebrewError> {
        let index = IndexStore::new(open_index(db)?);
        let (planned, _) = stored(&index, plan, confirm, clock.now())?;
        Ok(match check(brew, &planned)? {
            Check::Ready(observed) => observed.command().map(|command| Question {
                package: observed.package,
                command,
            }),
            Check::Refused(..) | Check::Changed(_) => None,
        })
    }

    /// Marks `plan` finished; a failure only warns, as nothing more runs
    /// under it.
    fn finish(index: &mut IndexStore, plan: PlanId, all_succeeded: bool) {
        if let Err(error) = index.apply_plan_event(plan, PlanEvent::Finish { all_succeeded }) {
            tracing::warn!(%error, "the Homebrew removal plan was not marked finished");
        }
    }

    pub(super) fn remove_with(
        brew: &dyn Homebrew,
        db: &Path,
        plan: PlanId,
        confirm: &str,
        surface: ConfirmationSurface,
        clock: &dyn Clock,
    ) -> Result<RemovalReport, HomebrewError> {
        let now = clock.now();
        let mut index = IndexStore::new(open_index(db)?);
        let (planned, plan_hash) = stored(&index, plan, confirm, now)?;
        // Confirmed once: a second run of the same preview is refused here.
        let confirmation = Confirmation {
            plan_hash,
            confirmed_at: now,
            surface,
        };
        index
            .apply_plan_event(plan, PlanEvent::Confirm { confirmation })
            .map_err(|error| match error {
                StoreError::PlanTransition(_) => QuarantineError::AlreadyConfirmed(plan).into(),
                other => HomebrewError::from(other),
            })?;
        index.apply_plan_event(plan, PlanEvent::Start)?;
        let report = |package, command, ran, result| RemovalReport {
            plan,
            package,
            command,
            ran,
            result,
        };
        let observed = match check(brew, &planned) {
            Ok(Check::Ready(observed)) => observed,
            Ok(Check::Refused(package, reasons)) => {
                finish(&mut index, plan, false);
                tracing::info!(reasons = reasons.len(), "Homebrew removal refused");
                let result = RemovalResult::Refused(reasons);
                return Ok(report(Some(package), None, None, result));
            }
            Ok(Check::Changed(package)) => {
                finish(&mut index, plan, false);
                return Ok(report(package, None, None, RemovalResult::Changed));
            }
            Err(error) => {
                finish(&mut index, plan, false);
                return Err(error);
            }
        };
        let (Some(command), Ok(bound), Some(program)) = (
            observed.command(),
            observed.ready.as_ref(),
            observed.package.program.as_ref(),
        ) else {
            finish(&mut index, plan, false);
            let package = Some(observed.package);
            return Ok(report(package, None, None, RemovalResult::Changed));
        };
        // Run once; never retried, whatever happens.
        let ran = match brew.run(program, &bound.args) {
            Ok(ran) => ran,
            Err(error) => {
                finish(&mut index, plan, false);
                return Err(HomebrewError::Run(error));
            }
        };
        // Success is observed: a fresh list, read whole, without it.
        let fresh = brew.packages();
        let listed = fresh
            .packages
            .iter()
            .any(|p| key(p).as_deref() == Some(observed.package.key.as_str()));
        let result = if listed {
            RemovalResult::StillListed
        } else if fresh.status == SourceStatus::Complete {
            RemovalResult::Removed
        } else {
            RemovalResult::Unverified
        };
        finish(&mut index, plan, result == RemovalResult::Removed);
        // The outcome only: never the output, which may name paths.
        tracing::info!(
            success = ran.success,
            timed_out = ran.timed_out,
            ?result,
            "Homebrew removal ran"
        );
        Ok(report(
            Some(observed.package),
            Some(command),
            Some(ran),
            result,
        ))
    }
}

#[cfg(target_os = "macos")]
mod mac {
    //! This Mac: Homebrew's records, the applications and the process table.

    use lumen_application::CancelToken;
    use lumen_application::ports::{InventoryReport, InventorySource as _};
    use lumen_domain::{PackageId, RawPath, UntrustedText};
    use lumen_platform_macos::{AppInventory, HomebrewInventory, ProcessProbe, management};

    use super::Ran;
    use super::unix::{Homebrew, native};
    use crate::tools::{TIME_LIMIT, ToolError};

    /// Homebrew's own switches: no auto-update or analytics before the
    /// uninstall, no hints, and plain output.
    const ENV: [(&str, &str); 5] = [
        ("HOMEBREW_NO_AUTO_UPDATE", "1"),
        ("HOMEBREW_NO_ANALYTICS", "1"),
        ("HOMEBREW_NO_ENV_HINTS", "1"),
        ("NO_COLOR", "1"),
        ("LC_ALL", "C"),
    ];

    pub(super) struct Mac {
        homebrew: HomebrewInventory,
    }

    impl Mac {
        pub(super) fn new() -> Self {
            Self {
                homebrew: HomebrewInventory::new(),
            }
        }
    }

    impl Homebrew for Mac {
        fn packages(&self) -> InventoryReport {
            self.homebrew.collect(&CancelToken::new())
        }

        fn applications(&self) -> InventoryReport {
            AppInventory::new().collect(&CancelToken::new())
        }

        fn rights(&self, package: &PackageId) -> Option<bool> {
            self.homebrew.removal_rights(package)
        }

        fn running(&self, folders: &[RawPath]) -> Option<bool> {
            let snapshot = ProcessProbe.snapshot();
            let mut result = Some(false);
            for folder in folders {
                match snapshot.running(folder) {
                    Some(true) => return Some(true),
                    Some(false) => {}
                    None => result = None,
                }
            }
            result
        }

        fn security_software(&self, bundle: &RawPath) -> Option<bool> {
            management::endpoint_security_client(&native(bundle), management::Run::AsCaller)
        }

        fn enrolled(&self) -> Option<bool> {
            management::mdm_enrolled()
        }

        fn managed(&self, bundle_id: &UntrustedText) -> Option<bool> {
            management::managed(bundle_id.raw())
        }

        fn run(&self, program: &RawPath, args: &[String]) -> Result<Ran, ToolError> {
            let home = std::env::home_dir().ok_or(ToolError::NoHome)?;
            let run = crate::tools::run_program_with_env(
                &native(program),
                args,
                &home,
                TIME_LIMIT,
                &ENV,
            )?;
            Ok(Ran {
                success: run.success,
                exit_code: run.exit_code,
                timed_out: run.timed_out,
                output: run.output,
            })
        }
    }
}

#[cfg(all(test, unix))]
#[allow(clippy::unwrap_used, clippy::panic, clippy::disallowed_methods)]
mod tests {
    //! Every test reads only its own temporary prefix and database; nothing
    //! is run.

    use std::cell::Cell;
    use std::path::{Path, PathBuf};

    use lumen_application::homebrew_removal::{Blocked, is_removal};
    use lumen_application::ports::InventoryReport;
    use lumen_domain::{
        AppId, AppIdScheme, Application, ConfirmationSurface, Package, PackageId, PackageManager,
        PackageRelation, PackageRelationKind, RawPath, RelationMethod, SourceName, SourceStatus,
        UntrustedText,
    };
    use lumen_store_sqlite::{IndexStore, open_index};

    use super::unix::{Homebrew, preview_with, question_with, remove_with};
    use super::*;
    use crate::tools::ToolError;
    use crate::{SystemClock, UuidV7Ids};

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn raw(path: &Path) -> RawPath {
        crate::raw_path(path).unwrap()
    }

    /// A prefix with `brew`, a formula `act`, a formula `jq` and a cask
    /// `hot` whose app is `Hot.app`.
    struct Fake {
        _dir: lumen_testkit::Fixture,
        prefix: PathBuf,
        db: PathBuf,
        status: Cell<SourceStatus>,
        /// `brew` removes `act` when it runs.
        removes: bool,
        /// How `brew` ends: its success and whether it was stopped.
        ends: Cell<(bool, bool)>,
        /// Homebrew's list after `brew` ran.
        after: Cell<SourceStatus>,
        /// `act` is gone.
        gone: Cell<bool>,
        /// Every run: the program and its arguments.
        runs: std::cell::RefCell<Vec<(RawPath, Vec<String>)>>,
        /// Whether `jq` needs `act`.
        needed: bool,
        rights: Cell<Option<bool>>,
        running: Cell<Option<bool>>,
        security: Cell<Option<bool>>,
        enrolled: Cell<Option<bool>>,
        /// The bundle ID `Hot.app` is listed with, if it is listed.
        bundle: Option<&'static str>,
    }

    impl Fake {
        fn new() -> Result<Self, Box<dyn std::error::Error>> {
            use std::os::unix::fs::PermissionsExt as _;
            let dir = lumen_testkit::Fixture::build(&[])?;
            let prefix = dir.root().join("brew");
            std::fs::create_dir_all(prefix.join("Cellar/act/0.2.79"))?;
            std::fs::create_dir_all(prefix.join("Cellar/jq/1.7"))?;
            std::fs::create_dir_all(prefix.join("Caskroom/hot/1.0"))?;
            std::fs::create_dir_all(prefix.join("bin"))?;
            std::fs::create_dir_all(dir.root().join("Applications/Hot.app"))?;
            let brew = prefix.join("bin/brew");
            std::fs::write(&brew, "")?;
            std::fs::set_permissions(&brew, std::fs::Permissions::from_mode(0o755))?;
            let db = dir.root().join("index.db");
            Ok(Self {
                prefix,
                db,
                _dir: dir,
                status: Cell::new(SourceStatus::Complete),
                removes: true,
                ends: Cell::new((true, false)),
                after: Cell::new(SourceStatus::Complete),
                gone: Cell::new(false),
                runs: std::cell::RefCell::new(Vec::new()),
                needed: false,
                rights: Cell::new(Some(true)),
                running: Cell::new(Some(false)),
                security: Cell::new(Some(false)),
                enrolled: Cell::new(Some(false)),
                bundle: Some("dev.hot.Hot"),
            })
        }

        fn app(&self) -> PathBuf {
            self.prefix.with_file_name("Applications").join("Hot.app")
        }

        fn package(&self, manager: PackageManager, name: &str, folder: &str) -> Package {
            Package {
                manager,
                id: UntrustedText::new(name),
                version: Some(UntrustedText::new("1.0")),
                install_root: Some(raw(&self.prefix.join(folder).join(name))),
                required: false,
                explicit: Some(true),
                relations: Some(Vec::new()),
            }
        }

        fn preview(&self, key: &str) -> Result<HomebrewPreview, HomebrewError> {
            preview_with(self, &self.db, key, &SystemClock, &UuidV7Ids)
        }
    }

    impl Homebrew for Fake {
        fn packages(&self) -> InventoryReport {
            let act = self.package(PackageManager::Homebrew, "act", "Cellar");
            let mut hot = self.package(PackageManager::HomebrewCask, "hot", "Caskroom");
            hot.relations = Some(vec![PackageRelation {
                kind: PackageRelationKind::Installed {
                    location: raw(&self.app()),
                },
                method: RelationMethod::CaskUninstallArtifacts,
            }]);
            let mut jq = self.package(PackageManager::Homebrew, "jq", "Cellar");
            if self.needed {
                jq.relations = Some(vec![PackageRelation {
                    kind: PackageRelationKind::DependsOn {
                        package: act.package_id(),
                        direct: Some(true),
                    },
                    method: RelationMethod::HomebrewRuntimeDependencies,
                }]);
            }
            InventoryReport {
                source: SourceName::new("test.homebrew").unwrap(),
                status: self.status.get(),
                applications: Vec::new(),
                services: Vec::new(),
                packages: if self.gone.get() {
                    vec![hot, jq]
                } else {
                    vec![act, hot, jq]
                },
            }
        }

        fn applications(&self) -> InventoryReport {
            let applications = self
                .bundle
                .map(|id| Application {
                    ids: [AppId {
                        scheme: AppIdScheme::BundleId,
                        value: UntrustedText::new(id),
                    }]
                    .into(),
                    name: None,
                    version: None,
                    vendor: None,
                    signature: None,
                    install_locations: vec![raw(&self.app())],
                    sources: std::collections::BTreeSet::new(),
                    uninstall: None,
                })
                .into_iter()
                .collect();
            InventoryReport {
                source: SourceName::new("test.apps").unwrap(),
                status: SourceStatus::Complete,
                applications,
                services: Vec::new(),
                packages: Vec::new(),
            }
        }

        fn rights(&self, _: &PackageId) -> Option<bool> {
            self.rights.get()
        }

        fn running(&self, _: &[RawPath]) -> Option<bool> {
            self.running.get()
        }

        fn security_software(&self, _: &RawPath) -> Option<bool> {
            self.security.get()
        }

        fn enrolled(&self) -> Option<bool> {
            self.enrolled.get()
        }

        fn managed(&self, _: &UntrustedText) -> Option<bool> {
            self.enrolled.get()
        }

        fn run(&self, program: &RawPath, args: &[String]) -> Result<Ran, ToolError> {
            self.runs
                .borrow_mut()
                .push((program.clone(), args.to_vec()));
            self.gone.set(self.removes);
            self.status.set(self.after.get());
            let (success, timed_out) = self.ends.get();
            Ok(Ran {
                success,
                exit_code: (!timed_out).then_some(i32::from(!success)),
                timed_out,
                output: "Uninstalling act... (12 files)".into(),
            })
        }
    }

    fn reasons(preview: &HomebrewPreview) -> Vec<Blocked> {
        preview.ready.clone().err().unwrap_or_default()
    }

    #[test]
    fn a_removable_package_is_stored_with_its_exact_command_in_its_own_prefix() -> TestResult {
        let fake = Fake::new()?;
        let preview = fake.preview("Homebrew:act")?;
        let Ok(ready) = &preview.ready else {
            panic!("{preview:?}")
        };
        let brew = fake.prefix.join("bin/brew").display().to_string();
        assert_eq!(
            ready.command,
            [brew.as_str(), "uninstall", "--formula", "act"]
        );
        assert_eq!(
            preview.package.folder,
            Some(raw(&fake.prefix.join("Cellar/act")))
        );
        let stored = IndexStore::new(open_index(&fake.db)?)
            .plan(ready.plan)?
            .unwrap();
        assert_eq!(stored.plan.hash(), ready.hash);
        assert!(is_removal(&stored.decisions));
        // A cask names its app, which goes with it.
        let cask = fake.preview("HomebrewCask:hot")?;
        let Ok(ready) = &cask.ready else {
            panic!("{cask:?}")
        };
        assert_eq!(ready.command[1..], ["uninstall", "--cask", "hot"]);
        assert_eq!(cask.package.apps, [raw(&fake.app())]);
        Ok(())
    }

    #[test]
    fn a_preview_is_never_run_as_a_clean_up() -> TestResult {
        let fake = Fake::new()?;
        let Ok(ready) = fake.preview("Homebrew:act")?.ready else {
            panic!("not ready")
        };
        let cleanup = crate::quarantine::quarantine(
            &fake.db,
            ready.plan,
            &ready.hash.to_string(),
            ConfirmationSurface::CommandLine,
            &SystemClock,
            &UuidV7Ids,
        );
        assert!(
            matches!(cleanup, Err(QuarantineError::NotACleanup(_))),
            "{cleanup:?}"
        );
        assert!(fake.prefix.join("Cellar/act/0.2.79").is_dir());
        Ok(())
    }

    #[test]
    fn every_failed_or_unknown_check_refuses_and_stores_nothing() -> TestResult {
        type Change = fn(&mut Fake);
        let cases: [(&str, Change, Blocked); 10] = [
            (
                "Homebrew:act",
                |f| f.rights.set(Some(false)),
                Blocked::NeedsAdministrator,
            ),
            (
                "Homebrew:act",
                |f| f.rights.set(None),
                Blocked::AdministratorUnknown,
            ),
            (
                "Homebrew:act",
                |f| f.running.set(Some(true)),
                Blocked::Running,
            ),
            (
                "Homebrew:act",
                |f| f.running.set(None),
                Blocked::RunningUnknown,
            ),
            (
                "Homebrew:act",
                |f| f.enrolled.set(Some(true)),
                Blocked::Managed,
            ),
            ("Homebrew:act", |f| f.needed = true, Blocked::Dependents),
            (
                "Homebrew:act",
                |f| f.status.set(SourceStatus::Partial),
                Blocked::InventoryIncomplete,
            ),
            (
                "HomebrewCask:hot",
                |f| f.security.set(Some(true)),
                Blocked::SecuritySoftware,
            ),
            (
                "HomebrewCask:hot",
                |f| f.security.set(None),
                Blocked::SecurityUnknown,
            ),
            (
                "HomebrewCask:hot",
                |f| f.bundle = None,
                Blocked::ManagedUnknown,
            ),
        ];
        for (key, change, reason) in cases {
            let mut fake = Fake::new()?;
            change(&mut fake);
            let preview = fake.preview(key)?;
            assert!(
                reasons(&preview).contains(&reason),
                "{reason:?}: {preview:?}"
            );
            assert!(!fake.db.exists(), "{reason:?}: stored");
        }
        Ok(())
    }

    #[test]
    fn without_brew_in_its_prefix_or_its_folder_it_is_not_removed() -> TestResult {
        let fake = Fake::new()?;
        std::fs::remove_file(fake.prefix.join("bin/brew"))?;
        let preview = fake.preview("Homebrew:act")?;
        assert_eq!(reasons(&preview), [Blocked::AdministratorUnknown]);
        let fake = Fake::new()?;
        std::fs::remove_dir_all(fake.prefix.join("Caskroom/hot"))?;
        let preview = fake.preview("HomebrewCask:hot")?;
        assert_eq!(reasons(&preview), [Blocked::AdministratorUnknown]);
        assert!(!fake.db.exists());
        Ok(())
    }

    #[test]
    fn a_key_homebrew_does_not_list_is_unknown() -> TestResult {
        let fake = Fake::new()?;
        for key in ["Homebrew:missing", "MacApp:act", "HomebrewCask:act", "act"] {
            assert!(
                matches!(fake.preview(key), Err(HomebrewError::UnknownPackage)),
                "{key}"
            );
        }
        Ok(())
    }

    /// The ready preview of `key`: its ID and hash.
    fn ready(fake: &Fake, key: &str) -> Result<(PlanId, String), Box<dyn std::error::Error>> {
        match fake.preview(key)?.ready {
            Ok(ready) => Ok((ready.plan, ready.hash.to_string())),
            Err(reasons) => Err(format!("refused: {reasons:?}").into()),
        }
    }

    fn remove(fake: &Fake, plan: PlanId, hash: &str) -> Result<RemovalReport, HomebrewError> {
        remove_with(
            fake,
            &fake.db,
            plan,
            hash,
            ConfirmationSurface::DesktopApp,
            &SystemClock,
        )
    }

    #[test]
    fn a_confirmed_preview_runs_its_exact_command_once_and_is_removed_only_when_gone() -> TestResult
    {
        let fake = Fake::new()?;
        let (plan, hash) = ready(&fake, "Homebrew:act")?;
        let question = question_with(&fake, &fake.db, plan, &hash, &SystemClock)?;
        let brew = fake.prefix.join("bin/brew");
        let command = vec![
            brew.display().to_string(),
            "uninstall".into(),
            "--formula".into(),
            "act".into(),
        ];
        assert_eq!(question.map(|q| q.command), Some(command.clone()));
        assert!(fake.runs.borrow().is_empty(), "asking runs nothing");
        let report = remove(&fake, plan, &hash)?;
        assert_eq!(report.result, RemovalResult::Removed);
        assert_eq!(report.command, Some(command));
        assert_eq!(
            *fake.runs.borrow(),
            [(
                raw(&brew),
                vec!["uninstall".into(), "--formula".into(), "act".into()]
            )]
        );
        // Confirmed once: the same preview never runs again.
        assert!(matches!(
            remove(&fake, plan, &hash),
            Err(HomebrewError::Plan(error)) if matches!(*error, QuarantineError::AlreadyConfirmed(_))
        ));
        assert_eq!(fake.runs.borrow().len(), 1);
        Ok(())
    }

    #[test]
    fn a_failed_or_stopped_brew_reports_what_remains_and_is_not_retried() -> TestResult {
        for ends in [(false, false), (false, true), (true, false)] {
            let mut fake = Fake::new()?;
            fake.removes = false;
            fake.ends.set(ends);
            let (plan, hash) = ready(&fake, "Homebrew:act")?;
            let report = remove(&fake, plan, &hash)?;
            assert_eq!(report.result, RemovalResult::StillListed, "{ends:?}");
            let ran = report.ran.ok_or("ran")?;
            assert_eq!((ran.success, ran.timed_out), ends);
            assert_eq!(fake.runs.borrow().len(), 1, "{ends:?}: run once");
        }
        // Gone, but the fresh list was not read whole: not counted as removed.
        let fake = Fake::new()?;
        fake.after.set(SourceStatus::Partial);
        let (plan, hash) = ready(&fake, "Homebrew:act")?;
        assert_eq!(
            remove(&fake, plan, &hash)?.result,
            RemovalResult::Unverified
        );
        Ok(())
    }

    #[test]
    fn nothing_runs_for_a_wrong_hash_a_changed_package_or_a_refusal_now() -> TestResult {
        let fake = Fake::new()?;
        let (plan, hash) = ready(&fake, "Homebrew:act")?;
        let wrong = format!("{}0", &hash[..hash.len() - 1]);
        let wrong = if wrong == hash {
            format!("{}1", &hash[..hash.len() - 1])
        } else {
            wrong
        };
        assert!(matches!(
            remove(&fake, plan, &wrong),
            Err(HomebrewError::Plan(error)) if matches!(*error, QuarantineError::Mismatch(_))
        ));
        // Something now runs from it: refused at the gate.
        fake.running.set(Some(true));
        let report = remove(&fake, plan, &hash)?;
        assert_eq!(
            report.result,
            RemovalResult::Refused(vec![Blocked::Running])
        );
        // Its folder replaced since the preview: a changed package.
        let fake = Fake::new()?;
        let (plan, hash) = ready(&fake, "Homebrew:act")?;
        let folder = fake.prefix.join("Cellar/act");
        std::fs::rename(&folder, fake.prefix.join("Cellar/act-old"))?;
        std::fs::create_dir_all(folder.join("0.2.79"))?;
        assert_eq!(remove(&fake, plan, &hash)?.result, RemovalResult::Changed);
        assert!(
            matches!(
                question_with(&fake, &fake.db, plan, &hash, &SystemClock),
                Ok(None)
            ),
            "nothing to confirm for a changed package"
        );
        assert!(fake.runs.borrow().is_empty());
        Ok(())
    }

    #[test]
    fn only_a_homebrew_preview_runs_as_one() -> TestResult {
        let fake = Fake::new()?;
        let (plan, _) = ready(&fake, "Homebrew:act")?;
        let mut index = IndexStore::new(open_index(&fake.db)?);
        let other = lumen_domain::CleanupPlan::new(
            IdGenerator::plan_id(&UuidV7Ids),
            SystemClock.now(),
            Vec::new(),
        );
        index.save_plan(&other, &[])?;
        assert!(matches!(
            remove(&fake, other.id(), &other.hash().to_string()),
            Err(HomebrewError::NotARemoval(_))
        ));
        assert_ne!(other.id(), plan);
        assert!(fake.runs.borrow().is_empty());
        Ok(())
    }
}
