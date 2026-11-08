//! Uninstalling a macOS application into quarantine (ADR-0062): what the
//! preview shows, and what became of the app. The page names an app by its
//! software-list key and confirms a preview by its ID and hash; no view
//! carries a path to act on (ADR-0023). Every name and path is
//! display-escaped untrusted text. A running app can be asked to quit
//! (roadmap 16.3a), named by its key too.

use lumen_app::uninstall::{
    QuitResult, UninstallApp, UninstallError, UninstallPreview, UninstallReport, UninstallResult,
};
use lumen_application::uninstall::Ineligible;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use super::{FailureView, LocationView};

/// Why an application cannot be uninstalled into quarantine. Each `*_unknown`
/// reason is a check Lumen could not answer, which never counts as passing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum UninstallReasonView {
    /// It is not directly in `/Applications` or `~/Applications`.
    NotInApplications,
    /// It is part of macOS (`/System`, `/Library/Apple`).
    Protected,
    /// It is not an application folder (a link or a file).
    NotABundle,
    /// Who owns it could not be read.
    OwnerUnknown,
    /// Another account owns it: an administrator installer (`.pkg`) or the
    /// App Store installed it.
    NotOwnedByUser,
    /// The background items could not all be read.
    ServicesUnknown,
    /// A background item belongs to it.
    ServiceLinked,
    /// Whether it has helpers registered with the system could not be read.
    HelpersUnknown,
    /// It has helpers registered with the system (login items, launch
    /// agents or daemons, privileged helpers, system extensions).
    HasHelpers,
    /// Whether it runs could not be known.
    RunningUnknown,
    /// It is running: quit it first.
    Running,
    /// Whether its drive has a quarantine store could not be known.
    StoreUnknown,
    /// Its drive has no quarantine store.
    NoStoreOnVolume,
}

impl From<Ineligible> for UninstallReasonView {
    fn from(reason: Ineligible) -> Self {
        match reason {
            Ineligible::NotInApplications => Self::NotInApplications,
            Ineligible::Protected => Self::Protected,
            Ineligible::NotABundle => Self::NotABundle,
            Ineligible::OwnerUnknown => Self::OwnerUnknown,
            Ineligible::NotOwnedByUser => Self::NotOwnedByUser,
            Ineligible::ServicesUnknown => Self::ServicesUnknown,
            Ineligible::ServiceLinked => Self::ServiceLinked,
            Ineligible::HelpersUnknown => Self::HelpersUnknown,
            Ineligible::HasHelpers => Self::HasHelpers,
            Ineligible::RunningUnknown => Self::RunningUnknown,
            Ineligible::Running => Self::Running,
            Ineligible::StoreUnknown => Self::StoreUnknown,
            Ineligible::NoStoreOnVolume => Self::NoStoreOnVolume,
        }
    }
}

fn reasons(reasons: &[Ineligible]) -> Vec<UninstallReasonView> {
    reasons
        .iter()
        .copied()
        .map(UninstallReasonView::from)
        .collect()
}

/// What happens to the app and its data when it is uninstalled this way.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum UninstallNoteView {
    /// It goes to Quarantine and can be restored until it is removed for
    /// good; it is never removed automatically.
    GoesToQuarantine,
    /// Its data in `~/Library` stays. After the next scan it appears in
    /// Explore as data of an app that is not installed.
    LibraryDataStays,
}

/// The application a preview is about, as observed now.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct UninstallAppView {
    /// The software list's key, as given.
    pub key: String,
    /// Its display name, if it has one; display-escaped.
    #[schema(required = true)]
    pub name: Option<String>,
    /// Its bundle identifier; display-escaped.
    pub bundle_id: String,
    /// Its version, if recorded; display-escaped.
    #[schema(required = true)]
    pub version: Option<String>,
    /// Where it is, for showing only; display-escaped.
    pub path: String,
    /// What removing it for good would free, in bytes.
    pub size_bytes: String,
    /// Every folder inside it was listed; otherwise the size is a lower
    /// bound.
    pub size_complete: bool,
    /// What happens to it and its data.
    pub notes: Vec<UninstallNoteView>,
}

impl From<&UninstallApp> for UninstallAppView {
    fn from(app: &UninstallApp) -> Self {
        Self {
            key: app.key.clone(),
            name: app.name.as_ref().map(lumen_domain::UntrustedText::display),
            bundle_id: app.bundle_id.display(),
            version: app
                .version
                .as_ref()
                .map(lumen_domain::UntrustedText::display),
            path: app.path.display(),
            size_bytes: app.reclaim.reclaimable_now.get().to_string(),
            size_complete: app.size_complete,
            notes: vec![
                UninstallNoteView::GoesToQuarantine,
                UninstallNoteView::LibraryDataStays,
            ],
        }
    }
}

/// What uninstalling an application into quarantine would do.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum UninstallPreviewView {
    /// It may be uninstalled: confirm `hash` to move it into Quarantine.
    Ready {
        /// The preview's ID, for `uninstall`.
        preview: String,
        /// Its full hash: the confirmation.
        hash: String,
        /// When it was made; it expires 24 hours later.
        #[schema(value_type = String, format = DateTime)]
        created_at: lumen_domain::Timestamp,
        /// The application.
        app: UninstallAppView,
    },
    /// It cannot be uninstalled this way; nothing was stored.
    Refused {
        /// The application.
        app: UninstallAppView,
        /// Every reason, at least one.
        reasons: Vec<UninstallReasonView>,
    },
    /// No installed application has that key now: list the software again.
    NotFound,
    /// More than one application has that key, so which one is meant is
    /// unknown: uninstall it in Finder.
    Ambiguous,
    /// Uninstalling into quarantine is only for macOS applications.
    Unsupported,
}

impl UninstallPreviewView {
    /// The view of a preview, or of the reason none was made.
    ///
    /// # Errors
    ///
    /// The error, if it is not one the page explains itself (a database
    /// failure).
    pub fn from_result(
        result: Result<UninstallPreview, UninstallError>,
    ) -> Result<Self, UninstallError> {
        match result {
            Ok(preview) => Ok(Self::from(&preview)),
            Err(UninstallError::UnknownApp) => Ok(Self::NotFound),
            Err(UninstallError::Ambiguous) => Ok(Self::Ambiguous),
            Err(UninstallError::Unsupported) => Ok(Self::Unsupported),
            Err(error) => Err(error),
        }
    }
}

impl From<&UninstallPreview> for UninstallPreviewView {
    fn from(preview: &UninstallPreview) -> Self {
        let app = UninstallAppView::from(&preview.app);
        match &preview.ready {
            Ok(ready) => Self::Ready {
                preview: ready.plan.to_string(),
                hash: ready.hash.to_string(),
                created_at: ready.created_at,
                app,
            },
            Err(why) => Self::Refused {
                app,
                reasons: reasons(why),
            },
        }
    }
}

/// What became of the application.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum UninstallResultView {
    /// It is in Quarantine, restorable.
    Quarantined {
        /// The operation, for restoring or removing it for good.
        operation: String,
    },
    /// Checked again right before the move, it may not go now; it was not
    /// moved.
    Refused {
        /// Every reason, at least one.
        reasons: Vec<UninstallReasonView>,
    },
    /// It is not what the preview showed; it was not moved. Preview again.
    Changed,
    /// The move failed.
    Failed {
        /// Why.
        reason: FailureView,
        /// Where the app is.
        left: LocationView,
    },
}

/// An executed uninstall preview.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct UninstallOutcomeView {
    /// The preview.
    pub preview: String,
    /// Where the app was, for showing only; display-escaped.
    pub path: String,
    /// What removing it for good frees, as previewed, in bytes.
    pub size_bytes: String,
    /// Interrupted operations from earlier runs that were resolved first.
    pub resolved: String,
    /// What happened.
    pub result: UninstallResultView,
    /// Problems that did not stop it.
    pub warnings: Vec<String>,
}

impl From<&UninstallReport> for UninstallOutcomeView {
    fn from(report: &UninstallReport) -> Self {
        Self {
            preview: report.plan.to_string(),
            path: report.path.display(),
            size_bytes: report.reclaim.get().to_string(),
            resolved: report.resolved.to_string(),
            result: match &report.result {
                UninstallResult::Quarantined(operation) => UninstallResultView::Quarantined {
                    operation: operation.to_string(),
                },
                UninstallResult::Refused(why) => UninstallResultView::Refused {
                    reasons: reasons(why),
                },
                UninstallResult::Changed => UninstallResultView::Changed,
                UninstallResult::Failed { reason, left } => UninstallResultView::Failed {
                    reason: (*reason).into(),
                    left: (*left).into(),
                },
            },
            warnings: report.warnings.clone(),
        }
    }
}

/// What asking a running app to quit did. It was asked the way its Quit menu
/// item asks, so it may ask to save or decline; preview it again to see
/// whether it still runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum QuitView {
    /// It was asked to quit.
    Asked,
    /// It is not running (any more).
    NotRunning,
    /// It is Lumen itself; it was not asked.
    Itself,
    /// The request could not be sent.
    Failed,
    /// No installed application has that key now: list the software again.
    NotFound,
    /// More than one application has that key, so which one is meant is
    /// unknown.
    Ambiguous,
    /// Asking an app to quit is only for macOS applications.
    Unsupported,
}

impl QuitView {
    /// The view of a quit request, or of the reason none was sent.
    ///
    /// # Errors
    ///
    /// The error, if it is not one the page explains itself (a privileged
    /// process).
    pub fn from_result(result: Result<QuitResult, UninstallError>) -> Result<Self, UninstallError> {
        match result {
            Ok(QuitResult::Asked) => Ok(Self::Asked),
            Ok(QuitResult::NotRunning) => Ok(Self::NotRunning),
            Ok(QuitResult::Itself) => Ok(Self::Itself),
            Ok(QuitResult::Failed) => Ok(Self::Failed),
            Err(UninstallError::UnknownApp) => Ok(Self::NotFound),
            Err(UninstallError::Ambiguous) => Ok(Self::Ambiguous),
            Err(UninstallError::Unsupported) => Ok(Self::Unsupported),
            Err(error) => Err(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use lumen_app::uninstall::Confirmable;
    use lumen_domain::{
        ByteCount, ItemLocation, QuarantineFailure, RawPath, ReclaimEstimate, Timestamp,
        UntrustedText,
    };

    use super::*;

    fn app() -> UninstallApp {
        UninstallApp {
            key: "MacApp:dev.zed.Zed".into(),
            bundle_id: UntrustedText::new("dev.zed.Zed"),
            name: Some(UntrustedText::new("Zed\u{202e}ppa")),
            version: None,
            path: RawPath::from_unix_bytes(b"/Applications/Z\xffed.app".to_vec())
                .unwrap_or_else(|_| unreachable!()),
            reclaim: ReclaimEstimate {
                reclaimable_now: ByteCount::new(u64::MAX),
                ..ReclaimEstimate::default()
            },
            size_complete: false,
        }
    }

    #[test]
    fn a_preview_is_tagged_escaped_and_counts_are_strings() {
        let ready = UninstallPreview {
            app: app(),
            ready: Ok(Confirmable {
                plan: "0190f0d2-6c1e-7a3b-8c4d-5e6f708192a3"
                    .parse()
                    .unwrap_or_else(|_| unreachable!()),
                hash: lumen_domain::PlanHash::from_bytes([0xab; 32]),
                created_at: Timestamp::UNIX_EPOCH,
            }),
        };
        let json = serde_json::to_value(UninstallPreviewView::from(&ready)).unwrap_or_default();
        assert_eq!(json["kind"], "ready");
        assert_eq!(json["preview"], "0190f0d2-6c1e-7a3b-8c4d-5e6f708192a3");
        assert!(json["hash"].as_str().is_some_and(|h| h.starts_with("b3:")));
        assert_eq!(json["app"]["size_bytes"], u64::MAX.to_string());
        assert_eq!(json["app"]["size_complete"], false);
        assert_eq!(
            json["app"]["notes"],
            serde_json::json!(["goes_to_quarantine", "library_data_stays"])
        );
        assert_eq!(json["app"]["name"], "Zed⟦U+202E⟧ppa");
        assert_eq!(json["app"]["path"], "/Applications/Z⟦0xFF⟧ed.app");

        let refused = UninstallPreview {
            app: app(),
            ready: Err(vec![Ineligible::NotOwnedByUser, Ineligible::RunningUnknown]),
        };
        let json = serde_json::to_value(UninstallPreviewView::from(&refused)).unwrap_or_default();
        assert_eq!(json["kind"], "refused");
        assert_eq!(
            json["reasons"],
            serde_json::json!(["not_owned_by_user", "running_unknown"])
        );
    }

    #[test]
    fn an_uninstall_names_an_app_by_the_software_lists_key() {
        let id = UntrustedText::new("dev.zed.Zed\u{7}");
        let app = lumen_domain::Application {
            ids: std::collections::BTreeSet::from([lumen_domain::AppId {
                scheme: lumen_domain::AppIdScheme::BundleId,
                value: id.clone(),
            }]),
            name: None,
            version: None,
            vendor: None,
            signature: None,
            install_locations: vec![app().path],
            sources: std::collections::BTreeSet::new(),
            uninstall: None,
        };
        let report = lumen_application::ports::InventoryReport {
            source: lumen_domain::SourceName::new("macos.applications")
                .unwrap_or_else(|_| unreachable!()),
            status: lumen_domain::SourceStatus::Complete,
            applications: vec![app],
            services: Vec::new(),
            packages: Vec::new(),
        };
        let listed = crate::views::SoftwareView::from_reports(&[report]);
        assert_eq!(
            listed.items.first().map(|item| item.key.clone()),
            Some(lumen_app::uninstall::key(&id))
        );
    }

    #[test]
    fn errors_the_page_explains_become_views() {
        for (error, kind) in [
            (UninstallError::UnknownApp, "not_found"),
            (UninstallError::Ambiguous, "ambiguous"),
            (UninstallError::Unsupported, "unsupported"),
        ] {
            let view = UninstallPreviewView::from_result(Err(error))
                .unwrap_or_else(|e| unreachable!("{e}"));
            assert_eq!(serde_json::to_value(view).unwrap_or_default()["kind"], kind);
        }
        let plan = "0190f0d2-6c1e-7a3b-8c4d-5e6f708192a3"
            .parse()
            .unwrap_or_else(|_| unreachable!());
        assert!(
            UninstallPreviewView::from_result(Err(UninstallError::NotAnUninstall(plan))).is_err()
        );
    }

    #[test]
    fn an_outcome_names_its_operation_or_where_the_app_is() {
        let plan = "0190f0d2-6c1e-7a3b-8c4d-5e6f708192a3"
            .parse()
            .unwrap_or_else(|_| unreachable!());
        let report = |result| UninstallReport {
            plan,
            resolved: 0,
            path: app().path,
            reclaim: ByteCount::new(42),
            result,
            warnings: Vec::new(),
        };
        let operation = "018f0000-0000-7000-8000-000000000001"
            .parse()
            .unwrap_or_else(|_| unreachable!());
        let json = |result| {
            serde_json::to_value(UninstallOutcomeView::from(&report(result))).unwrap_or_default()
        };
        let moved = json(UninstallResult::Quarantined(operation));
        assert_eq!(moved["result"]["kind"], "quarantined");
        assert_eq!(
            moved["result"]["operation"],
            "018f0000-0000-7000-8000-000000000001"
        );
        assert_eq!(moved["size_bytes"], "42");
        assert_eq!(json(UninstallResult::Changed)["result"]["kind"], "changed");
        let failed = json(UninstallResult::Failed {
            reason: QuarantineFailure::InUse,
            left: ItemLocation::Source,
        });
        assert_eq!(
            failed["result"],
            serde_json::json!({"kind": "failed", "reason": "in_use", "left": "source"})
        );
        let refused = json(UninstallResult::Refused(vec![Ineligible::Running]));
        assert_eq!(
            refused["result"],
            serde_json::json!({"kind": "refused", "reasons": ["running"]})
        );
    }

    #[test]
    fn a_quit_request_is_tagged_and_explained_errors_are_views() {
        let json = |result| {
            QuitView::from_result(result)
                .map(|view| serde_json::to_value(view).unwrap_or_default())
                .ok()
        };
        assert_eq!(
            json(Ok(QuitResult::Asked)),
            Some(serde_json::json!({"kind": "asked"}))
        );
        assert_eq!(
            json(Ok(QuitResult::NotRunning)),
            Some(serde_json::json!({"kind": "not_running"}))
        );
        assert_eq!(
            json(Ok(QuitResult::Itself)),
            Some(serde_json::json!({"kind": "itself"}))
        );
        assert_eq!(
            json(Err(UninstallError::UnknownApp)),
            Some(serde_json::json!({"kind": "not_found"}))
        );
        assert!(
            QuitView::from_result(Err(UninstallError::NotAnUninstall(
                "0190f0d2-6c1e-7a3b-8c4d-5e6f708192a3"
                    .parse()
                    .unwrap_or_else(|_| unreachable!())
            )))
            .is_err()
        );
    }
}
