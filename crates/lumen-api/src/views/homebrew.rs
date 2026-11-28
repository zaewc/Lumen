//! Removing a Homebrew formula or cask through Homebrew (ADR-0060, roadmap
//! 16.7): what the preview shows. The page names a package by its
//! software-list key and confirms a preview by its ID and hash; no view
//! carries a path or a command to act on (ADR-0023). The command is shown,
//! exactly as it will run. Every name and path is display-escaped untrusted
//! text.

use lumen_app::homebrew::{
    HomebrewError, HomebrewPackage, HomebrewPreview, RemovalReport, RemovalResult,
};
use lumen_application::homebrew_removal::Blocked;
use lumen_domain::PackageManager;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Why a Homebrew package is not removed from Lumen. Each `*_unknown` reason
/// is a check Lumen could not answer, which never counts as passing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum HomebrewReasonView {
    /// Its name is not a plain Homebrew name.
    UnsafeName,
    /// Homebrew does not list it now.
    NotListed,
    /// Homebrew lists it more than once.
    Ambiguous,
    /// Homebrew's list could not be read whole.
    InventoryIncomplete,
    /// It is protected or part of the system.
    Protected,
    /// Another installed package needs it.
    Dependents,
    /// Whether anything needs it could not be told.
    DependentsUnknown,
    /// Removing it needs administrator rights.
    NeedsAdministrator,
    /// Whether removing it needs administrator rights could not be told.
    AdministratorUnknown,
    /// It is security software.
    SecuritySoftware,
    /// Whether it is security software could not be told.
    SecurityUnknown,
    /// An organization may manage it.
    Managed,
    /// Whether an organization manages it could not be told.
    ManagedUnknown,
    /// Something it installed is running: quit it first.
    Running,
    /// Whether something it installed is running could not be told.
    RunningUnknown,
    /// Another reason Lumen keeps it.
    Other,
}

impl From<Blocked> for HomebrewReasonView {
    fn from(reason: Blocked) -> Self {
        match reason {
            Blocked::UnsafeName => Self::UnsafeName,
            Blocked::NotListed => Self::NotListed,
            Blocked::Ambiguous => Self::Ambiguous,
            Blocked::InventoryIncomplete => Self::InventoryIncomplete,
            Blocked::Protected => Self::Protected,
            Blocked::Dependents => Self::Dependents,
            Blocked::DependentsUnknown => Self::DependentsUnknown,
            Blocked::NeedsAdministrator => Self::NeedsAdministrator,
            Blocked::AdministratorUnknown => Self::AdministratorUnknown,
            Blocked::SecuritySoftware => Self::SecuritySoftware,
            Blocked::SecurityUnknown => Self::SecurityUnknown,
            Blocked::Managed => Self::Managed,
            Blocked::ManagedUnknown => Self::ManagedUnknown,
            Blocked::Running => Self::Running,
            Blocked::RunningUnknown => Self::RunningUnknown,
            // Not Homebrew, or a reason added later: kept all the same.
            _ => Self::Other,
        }
    }
}

/// What removing it through Homebrew does and does not do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum HomebrewNoteView {
    /// Lumen cannot undo it: nothing goes to Quarantine (ADR-0060 rule 5).
    CannotBeUndone,
    /// Its data in the home folder stays: Lumen never asks Homebrew to
    /// `--zap`.
    UserDataStays,
    /// The packages it needs stay installed.
    DependenciesStay,
}

/// Whether it is a formula or a cask.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum HomebrewKindView {
    /// A formula.
    Formula,
    /// A cask.
    Cask,
}

/// The package a preview is about, as observed now.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct HomebrewPackageView {
    /// The software list's key, as given.
    pub key: String,
    /// A formula or a cask.
    pub kind: HomebrewKindView,
    /// Its name; display-escaped.
    pub name: String,
    /// Its version, if listed; display-escaped.
    #[schema(required = true)]
    pub version: Option<String>,
    /// The apps a cask placed, which go with it, for showing only;
    /// display-escaped.
    pub apps: Vec<String>,
    /// What removing it does and does not do.
    pub notes: Vec<HomebrewNoteView>,
}

impl From<&HomebrewPackage> for HomebrewPackageView {
    fn from(package: &HomebrewPackage) -> Self {
        Self {
            key: package.key.clone(),
            kind: if package.package.manager == PackageManager::HomebrewCask {
                HomebrewKindView::Cask
            } else {
                HomebrewKindView::Formula
            },
            name: package.package.id.display(),
            version: package
                .version
                .as_ref()
                .map(lumen_domain::UntrustedText::display),
            apps: package
                .apps
                .iter()
                .map(lumen_domain::RawPath::display)
                .collect(),
            notes: vec![
                HomebrewNoteView::CannotBeUndone,
                HomebrewNoteView::UserDataStays,
                HomebrewNoteView::DependenciesStay,
            ],
        }
    }
}

/// What removing a Homebrew package would do.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum HomebrewPreviewView {
    /// It may be removed: confirm `hash` to run `command`.
    Ready {
        /// The preview's ID.
        preview: String,
        /// Its full hash: the confirmation.
        hash: String,
        /// When it was made; it expires 24 hours later.
        #[schema(value_type = String, format = DateTime)]
        created_at: lumen_domain::Timestamp,
        /// The package.
        package: HomebrewPackageView,
        /// The exact command, for showing only: the program by absolute
        /// path, then its arguments. The page never sends it back.
        command: Vec<String>,
    },
    /// Lumen does not remove it; nothing was stored.
    Refused {
        /// The package.
        package: HomebrewPackageView,
        /// Every reason, at least one.
        reasons: Vec<HomebrewReasonView>,
    },
    /// Homebrew lists no package with that key now: list the software again.
    NotFound,
    /// Removing a Homebrew package is only for macOS.
    Unsupported,
}

impl HomebrewPreviewView {
    /// The view of a preview, or of the reason none was made.
    ///
    /// # Errors
    ///
    /// The error, if it is not one the page explains itself (a database
    /// failure).
    pub fn from_result(
        result: Result<HomebrewPreview, HomebrewError>,
    ) -> Result<Self, HomebrewError> {
        match result {
            Ok(preview) => Ok(Self::from(&preview)),
            Err(HomebrewError::UnknownPackage) => Ok(Self::NotFound),
            Err(HomebrewError::Unsupported) => Ok(Self::Unsupported),
            Err(error) => Err(error),
        }
    }
}

impl From<&HomebrewPreview> for HomebrewPreviewView {
    fn from(preview: &HomebrewPreview) -> Self {
        let package = HomebrewPackageView::from(&preview.package);
        match &preview.ready {
            Ok(ready) => Self::Ready {
                preview: ready.plan.to_string(),
                hash: ready.hash.to_string(),
                created_at: ready.created_at,
                package,
                command: ready.command.clone(),
            },
            Err(why) => {
                let mut reasons: Vec<HomebrewReasonView> =
                    why.iter().copied().map(HomebrewReasonView::from).collect();
                reasons.dedup();
                Self::Refused { package, reasons }
            }
        }
    }
}

/// What became of the package.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum HomebrewResultView {
    /// A fresh Homebrew list, read whole, no longer lists it: removed.
    Removed,
    /// A fresh Homebrew list still lists it.
    StillListed,
    /// A fresh Homebrew list could not be read whole, so whether it is gone
    /// is not known.
    Unverified,
    /// Checked again right before running, Lumen keeps it; nothing ran.
    Refused {
        /// Every reason, at least one.
        reasons: Vec<HomebrewReasonView>,
    },
    /// It is not what the preview showed; nothing ran. Preview it again.
    Changed,
}

/// How `brew` ended, as it reported.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct HomebrewRunView {
    /// It exited with status 0.
    pub success: bool,
    /// Its exit code, if it exited on its own.
    #[schema(required = true)]
    pub exit_code: Option<i32>,
    /// It ran past the time limit and was stopped.
    pub timed_out: bool,
    /// The end of what it printed; untrusted text, shown as is.
    pub output: String,
}

/// What running a Homebrew removal preview did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct HomebrewOutcomeView {
    /// The preview.
    pub preview: String,
    /// The package, if it was found right before running.
    #[schema(required = true)]
    pub package: Option<HomebrewPackageView>,
    /// The command run, if it ran.
    #[schema(required = true)]
    pub command: Option<Vec<String>>,
    /// How `brew` ended, if it ran.
    #[schema(required = true)]
    pub ran: Option<HomebrewRunView>,
    /// What became of the package.
    pub result: HomebrewResultView,
}

impl From<&RemovalReport> for HomebrewOutcomeView {
    fn from(report: &RemovalReport) -> Self {
        Self {
            preview: report.plan.to_string(),
            package: report.package.as_ref().map(HomebrewPackageView::from),
            command: report.command.clone(),
            ran: report.ran.as_ref().map(|ran| HomebrewRunView {
                success: ran.success,
                exit_code: ran.exit_code,
                timed_out: ran.timed_out,
                output: ran.output.clone(),
            }),
            result: match &report.result {
                RemovalResult::Removed => HomebrewResultView::Removed,
                RemovalResult::StillListed => HomebrewResultView::StillListed,
                RemovalResult::Unverified => HomebrewResultView::Unverified,
                RemovalResult::Refused(why) => HomebrewResultView::Refused {
                    reasons: why.iter().copied().map(HomebrewReasonView::from).collect(),
                },
                RemovalResult::Changed => HomebrewResultView::Changed,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use lumen_domain::{PackageId, UntrustedText};

    use super::*;

    fn package(manager: PackageManager) -> HomebrewPackage {
        HomebrewPackage {
            key: "HomebrewCask:hot".into(),
            package: PackageId {
                manager,
                id: UntrustedText::new("hot"),
            },
            version: Some(UntrustedText::new("1.9.4")),
            folder: None,
            program: None,
            apps: Vec::new(),
        }
    }

    #[test]
    fn a_refusal_names_every_reason_and_says_it_cannot_be_undone() {
        let view = HomebrewPreviewView::from(&HomebrewPreview {
            package: package(PackageManager::HomebrewCask),
            ready: Err(vec![Blocked::NotHomebrew, Blocked::RunningUnknown]),
        });
        let HomebrewPreviewView::Refused { package, reasons } = view else {
            unreachable!("refused")
        };
        assert_eq!(package.kind, HomebrewKindView::Cask);
        assert_eq!(
            reasons,
            [
                HomebrewReasonView::Other,
                HomebrewReasonView::RunningUnknown
            ]
        );
        assert!(package.notes.contains(&HomebrewNoteView::CannotBeUndone));
        assert!(package.notes.contains(&HomebrewNoteView::UserDataStays));
    }

    #[test]
    fn errors_the_page_explains_become_views() {
        assert!(matches!(
            HomebrewPreviewView::from_result(Err(HomebrewError::UnknownPackage)),
            Ok(HomebrewPreviewView::NotFound)
        ));
        assert!(matches!(
            HomebrewPreviewView::from_result(Err(HomebrewError::Unsupported)),
            Ok(HomebrewPreviewView::Unsupported)
        ));
    }
}
