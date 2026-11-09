//! The installed software list (ADR-0060 step 3): one row per application or
//! package that a platform inventory reports, with where the record came from
//! and how completely each inventory was read. Background items are not rows:
//! each is shown under the software it is linked to by evidence (ADR-0060 step
//! 2, [`lumen_application::software_links`]). Read only; nothing here removes
//! anything.
//!
//! While AI review is on, the same rows are what AI review may explain
//! (ADR-0061): [`software_subjects`] builds each one's request from the raw
//! inventory records, with identity fields only, and [`SoftwareView::with_notes`]
//! shows the cached notes that may be shown.

use lumen_app::software_notes::{NoteCache, SoftwareKey};
use lumen_application::ports::InventoryReport;
use lumen_application::software_links::{LinkMethod, link_services};
use lumen_application::software_notes::SoftwareSubject;
use lumen_domain::{
    AppIdScheme, Application, JudgmentConfidence, Package, PackageManager, RemovalImpact, Service,
    ServiceKind, ServiceScope, SoftwareFacts, SoftwareNote, SoftwareOrigin, SoftwarePurpose,
    SoftwareRequest, SoftwareService, SourceStatus,
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use super::ConfidenceView;

/// Which inventory recorded a piece of software, and so how it is identified.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SoftwareOriginView {
    /// A macOS application bundle, by bundle identifier.
    MacApp,
    /// A macOS installer package receipt, by package identifier.
    MacInstaller,
    /// A Homebrew formula.
    Homebrew,
    /// A Homebrew cask.
    HomebrewCask,
    /// A Windows program, by its Uninstall entry (or MSI product code).
    WindowsProgram,
    /// An MSIX/AppX package, by package family name.
    Msix,
    /// A dpkg package, by `name:arch`.
    Dpkg,
    /// An RPM package, by `name.arch`.
    Rpm,
    /// A pacman package.
    Pacman,
    /// A Flatpak application.
    Flatpak,
    /// A snap.
    Snap,
    /// Another source.
    Other,
}

/// How completely one inventory was read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SoftwareSourceStateView {
    /// Read whole.
    Complete,
    /// Read in part: some software may be missing from the list.
    Partial,
    /// Not on this computer, or not readable: absent from the list, not empty.
    Unavailable,
}

/// One inventory and how completely it was read.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct SoftwareSourceView {
    /// The inventory's name, such as `macos.homebrew`.
    pub source: String,
    /// How completely it was read.
    pub state: SoftwareSourceStateView,
    /// How many rows, or background items, it gave.
    pub count: u32,
}

/// What kind of background item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ServiceKindView {
    /// A macOS launch agent.
    LaunchAgent,
    /// A macOS launch daemon.
    LaunchDaemon,
    /// A macOS login item.
    LoginItem,
    /// A Windows service.
    WindowsService,
    /// A Windows driver.
    Driver,
    /// A scheduled task.
    ScheduledTask,
    /// A Windows startup entry.
    StartupEntry,
    /// Another kind.
    Other,
}

/// Whether a background item runs for one user or for the whole system.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ServiceScopeView {
    /// Runs as the user.
    User,
    /// Runs for the whole system.
    System,
}

/// The evidence linking a background item to a piece of software.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ServiceLinkMethodView {
    /// The program it runs lies inside the software's install location.
    ProgramInsideApplication,
    /// Its launchd label starts with the bundle identifier: a naming
    /// convention, never proof.
    LabelMatchesBundleId,
}

/// A background item linked to a piece of software.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct LinkedServiceView {
    /// What kind of item.
    pub kind: ServiceKindView,
    /// Its label, service name or entry name; display-escaped.
    pub label: String,
    /// User or system.
    pub scope: ServiceScopeView,
    /// The evidence for the link.
    pub method: ServiceLinkMethodView,
    /// The link is proven: its program lies inside this software and no other.
    pub verified: bool,
    /// It matched other software too: this is one candidate, not the owner.
    pub ambiguous: bool,
}

/// What AI review says a piece of software is mainly for (ADR-0061).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SoftwarePurposeView {
    /// A web browser.
    WebBrowser,
    /// Messaging, email or calls.
    Communication,
    /// Music, video or photos.
    Media,
    /// Documents, notes or other office work.
    OfficeOrProductivity,
    /// A developer tool.
    DevelopmentTool,
    /// Design or other creative work.
    DesignOrCreative,
    /// A game or game launcher.
    Game,
    /// Security, banking or identity.
    SecurityOrBanking,
    /// Part of the system, a driver or hardware support.
    SystemOrDriver,
    /// Cloud storage, sync or backup.
    CloudSyncOrBackup,
    /// A utility for managing the computer.
    Utility,
    /// A runtime or library other software uses.
    RuntimeOrLibrary,
    /// AI review could not tell.
    Unknown,
}

/// What AI review says removing a piece of software could affect besides
/// itself (ADR-0061): a description, never a statement that removal is safe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum RemovalImpactView {
    /// Mainly its own features.
    OnlyThisApp,
    /// Other software may depend on it.
    OtherSoftwareDepends,
    /// Its background items would stop.
    StopsBackgroundActivity,
    /// Security, banking or sign-in may stop working.
    SecurityOrAccess,
    /// An operating-system feature may stop working.
    SystemFunction,
    /// AI review could not tell.
    Unknown,
}

/// An AI note on a piece of software: closed choices and a confidence, never
/// model text. Display only (ADR-0061).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct SoftwareNoteView {
    /// What it is most likely for.
    pub purpose: SoftwarePurposeView,
    /// What removing it could most likely affect.
    pub removal_impact: RemovalImpactView,
    /// How sure AI review was.
    pub confidence: ConfidenceView,
}

impl From<SoftwareNote> for SoftwareNoteView {
    fn from(note: SoftwareNote) -> Self {
        Self {
            purpose: match note.purpose {
                SoftwarePurpose::WebBrowser => SoftwarePurposeView::WebBrowser,
                SoftwarePurpose::Communication => SoftwarePurposeView::Communication,
                SoftwarePurpose::Media => SoftwarePurposeView::Media,
                SoftwarePurpose::OfficeOrProductivity => SoftwarePurposeView::OfficeOrProductivity,
                SoftwarePurpose::DevelopmentTool => SoftwarePurposeView::DevelopmentTool,
                SoftwarePurpose::DesignOrCreative => SoftwarePurposeView::DesignOrCreative,
                SoftwarePurpose::Game => SoftwarePurposeView::Game,
                SoftwarePurpose::SecurityOrBanking => SoftwarePurposeView::SecurityOrBanking,
                SoftwarePurpose::SystemOrDriver => SoftwarePurposeView::SystemOrDriver,
                SoftwarePurpose::CloudSyncOrBackup => SoftwarePurposeView::CloudSyncOrBackup,
                SoftwarePurpose::Utility => SoftwarePurposeView::Utility,
                SoftwarePurpose::RuntimeOrLibrary => SoftwarePurposeView::RuntimeOrLibrary,
                SoftwarePurpose::Unknown => SoftwarePurposeView::Unknown,
            },
            removal_impact: match note.removal_impact {
                RemovalImpact::OnlyThisApp => RemovalImpactView::OnlyThisApp,
                RemovalImpact::OtherSoftwareDepends => RemovalImpactView::OtherSoftwareDepends,
                RemovalImpact::StopsBackgroundActivity => {
                    RemovalImpactView::StopsBackgroundActivity
                }
                RemovalImpact::SecurityOrAccess => RemovalImpactView::SecurityOrAccess,
                RemovalImpact::SystemFunction => RemovalImpactView::SystemFunction,
                RemovalImpact::Unknown => RemovalImpactView::Unknown,
            },
            confidence: match note.confidence {
                JudgmentConfidence::Low => ConfidenceView::Low,
                JudgmentConfidence::Medium => ConfidenceView::Medium,
                JudgmentConfidence::High => ConfidenceView::High,
            },
        }
    }
}

/// One application or package.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct SoftwareItemView {
    /// A key unique in the list: the origin and the native identifier.
    pub key: String,
    /// Which inventory recorded it.
    pub origin: SoftwareOriginView,
    /// Its native identifier, display-escaped.
    pub id: String,
    /// Its display name, if the record has one; display-escaped.
    #[schema(required = true)]
    pub name: Option<String>,
    /// Its version, as recorded; display-escaped.
    #[schema(required = true)]
    pub version: Option<String>,
    /// Its publisher or vendor, as recorded; display-escaped.
    #[schema(required = true)]
    pub vendor: Option<String>,
    /// Where it is installed, if recorded; display-escaped.
    #[schema(required = true)]
    pub location: Option<String>,
    /// Part of the operating system or marked by its platform as not
    /// removable: never removable from Lumen (ADR-0060).
    pub protected: bool,
    /// The platform's own list hides it (Windows `SystemComponent`).
    pub hidden: bool,
    /// Whether it was installed on request rather than as another package's
    /// dependency; `None` where the record does not say.
    #[schema(required = true)]
    pub explicit: Option<bool>,
    /// Background items linked to it, verified first.
    pub services: Vec<LinkedServiceView>,
    /// What AI review said about it, when it may be shown (ADR-0061).
    #[schema(required = true)]
    pub ai_note: Option<SoftwareNoteView>,
}

/// The installed software list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct SoftwareView {
    /// Every application and package, by name.
    pub items: Vec<SoftwareItemView>,
    /// Every inventory read, with how completely.
    pub sources: Vec<SoftwareSourceView>,
}

const fn state(status: SourceStatus) -> SoftwareSourceStateView {
    match status {
        SourceStatus::Complete => SoftwareSourceStateView::Complete,
        SourceStatus::Partial | SourceStatus::SuspectedTruncated => {
            SoftwareSourceStateView::Partial
        }
        SourceStatus::Unavailable => SoftwareSourceStateView::Unavailable,
    }
}

const fn service_kind(kind: ServiceKind) -> ServiceKindView {
    match kind {
        ServiceKind::LaunchAgent => ServiceKindView::LaunchAgent,
        ServiceKind::LaunchDaemon => ServiceKindView::LaunchDaemon,
        ServiceKind::LoginItem => ServiceKindView::LoginItem,
        ServiceKind::WindowsService => ServiceKindView::WindowsService,
        ServiceKind::Driver => ServiceKindView::Driver,
        ServiceKind::ScheduledTask => ServiceKindView::ScheduledTask,
        ServiceKind::StartupEntry => ServiceKindView::StartupEntry,
        _ => ServiceKindView::Other,
    }
}

fn linked(
    service: &Service,
    method: LinkMethod,
    verified: bool,
    ambiguous: bool,
) -> LinkedServiceView {
    LinkedServiceView {
        kind: service_kind(service.kind),
        label: service.label.display(),
        scope: match service.scope {
            ServiceScope::User => ServiceScopeView::User,
            ServiceScope::System => ServiceScopeView::System,
        },
        method: match method {
            LinkMethod::ProgramInsideApplication => ServiceLinkMethodView::ProgramInsideApplication,
            LinkMethod::LabelMatchesBundleId => ServiceLinkMethodView::LabelMatchesBundleId,
        },
        verified,
        ambiguous,
    }
}

const fn origin(manager: PackageManager) -> SoftwareOriginView {
    match manager {
        PackageManager::MacInstaller => SoftwareOriginView::MacInstaller,
        PackageManager::Homebrew => SoftwareOriginView::Homebrew,
        PackageManager::HomebrewCask => SoftwareOriginView::HomebrewCask,
        PackageManager::Msix => SoftwareOriginView::Msix,
        PackageManager::Dpkg => SoftwareOriginView::Dpkg,
        PackageManager::Rpm => SoftwareOriginView::Rpm,
        PackageManager::Pacman => SoftwareOriginView::Pacman,
        PackageManager::Flatpak => SoftwareOriginView::Flatpak,
        PackageManager::Snap => SoftwareOriginView::Snap,
        _ => SoftwareOriginView::Other,
    }
}

/// The raw identity fields a row's AI request is built from (ADR-0061 rule
/// 2): no path, user name, usage signal or size.
#[derive(Debug, Clone)]
struct Raw {
    id: String,
    name: Option<String>,
    publisher: Option<String>,
    version: Option<String>,
    services: Vec<SoftwareService>,
}

fn raw(text: Option<&lumen_domain::UntrustedText>) -> Option<String> {
    text.map(|t| t.raw().to_owned())
}

fn package(package: &Package) -> (SoftwareItemView, Raw) {
    let origin = origin(package.manager);
    let id = package.id.display();
    let raw = Raw {
        id: package.id.raw().to_owned(),
        name: None,
        publisher: None,
        version: raw(package.version.as_ref()),
        services: Vec::new(),
    };
    let view = SoftwareItemView {
        key: format!("{origin:?}:{id}"),
        origin,
        name: None,
        version: package
            .version
            .as_ref()
            .map(lumen_domain::UntrustedText::display),
        vendor: None,
        location: package
            .install_root
            .as_ref()
            .map(lumen_domain::RawPath::display),
        protected: package.required,
        hidden: false,
        explicit: package.explicit,
        services: Vec::new(),
        ai_note: None,
        id,
    };
    (view, raw)
}

/// The identifier an application is listed by: its product code for a
/// Windows Installer program, else the first identifier it has.
fn application(app: &Application) -> Option<(SoftwareItemView, Raw)> {
    let pick = |scheme| app.ids.iter().find(|id| id.scheme == scheme);
    let (origin, id) = [
        (AppIdScheme::BundleId, SoftwareOriginView::MacApp),
        (
            AppIdScheme::MsiProductCode,
            SoftwareOriginView::WindowsProgram,
        ),
        (
            AppIdScheme::UninstallKey,
            SoftwareOriginView::WindowsProgram,
        ),
        (AppIdScheme::MsixFamily, SoftwareOriginView::Msix),
    ]
    .into_iter()
    .find_map(|(scheme, origin)| pick(scheme).map(|id| (origin, &id.value)))?;
    let raw_id = id.raw().to_owned();
    let id = id.display();
    let location = app.install_locations.first();
    // macOS's own applications live in /System/Applications (and the Safari
    // cryptex), which the system volume's protection keeps read-only.
    let system_app = location.is_some_and(|l| {
        let bytes = l.as_bytes();
        bytes.starts_with(b"/System/") || bytes.starts_with(b"/Library/Apple/")
    });
    let record = app.uninstall.as_ref();
    let raw = Raw {
        id: raw_id,
        name: raw(app.name.as_ref()),
        publisher: raw(app.vendor.as_ref()),
        version: raw(app.version.as_ref()),
        services: Vec::new(),
    };
    let view = SoftwareItemView {
        key: format!("{origin:?}:{id}"),
        origin,
        name: app.name.as_ref().map(lumen_domain::UntrustedText::display),
        version: app
            .version
            .as_ref()
            .map(lumen_domain::UntrustedText::display),
        vendor: app
            .vendor
            .as_ref()
            .map(lumen_domain::UntrustedText::display),
        location: location.map(lumen_domain::RawPath::display),
        protected: system_app || record.is_some_and(|r| r.no_remove),
        hidden: record.is_some_and(|r| r.hidden),
        explicit: None,
        services: Vec::new(),
        ai_note: None,
        id,
    };
    Some((view, raw))
}

/// The domain's name for a row's origin.
const fn software_origin(origin: SoftwareOriginView) -> SoftwareOrigin {
    match origin {
        SoftwareOriginView::MacApp => SoftwareOrigin::MacApp,
        SoftwareOriginView::MacInstaller => SoftwareOrigin::MacInstaller,
        SoftwareOriginView::Homebrew => SoftwareOrigin::Homebrew,
        SoftwareOriginView::HomebrewCask => SoftwareOrigin::HomebrewCask,
        SoftwareOriginView::WindowsProgram => SoftwareOrigin::WindowsProgram,
        SoftwareOriginView::Msix => SoftwareOrigin::Msix,
        SoftwareOriginView::Dpkg => SoftwareOrigin::Dpkg,
        SoftwareOriginView::Rpm => SoftwareOrigin::Rpm,
        SoftwareOriginView::Pacman => SoftwareOrigin::Pacman,
        SoftwareOriginView::Flatpak => SoftwareOrigin::Flatpak,
        SoftwareOriginView::Snap => SoftwareOrigin::Snap,
        SoftwareOriginView::Other => SoftwareOrigin::Other,
    }
}

/// The cache key of a row: its origin, identifier and version as listed.
#[must_use]
pub fn software_key(item: &SoftwareItemView) -> SoftwareKey {
    SoftwareKey {
        origin: software_origin(item.origin),
        id: item.id.clone(),
        version: item.version.clone(),
    }
}

/// What AI review may be asked about each row of the list from `reports`
/// (ADR-0061), in the list's order: identity fields only, never a path.
/// Rows the list shows by default (installed on request or not known
/// otherwise, and not hidden) are preferred.
#[must_use]
pub fn software_subjects(reports: &[InventoryReport]) -> Vec<SoftwareSubject<SoftwareKey>> {
    rows(reports)
        .into_iter()
        .filter_map(|(item, raw)| {
            let request = SoftwareRequest::new(SoftwareFacts {
                origin: software_origin(item.origin),
                id: &raw.id,
                name: raw.name.as_deref(),
                publisher: raw.publisher.as_deref(),
                version: raw.version.as_deref(),
                protected: item.protected,
                hidden: item.hidden,
                services: raw.services,
            })
            .ok()?;
            Some(SoftwareSubject {
                key: software_key(&item),
                request,
                preferred: item.explicit != Some(false) && !item.hidden,
            })
        })
        .collect()
}

/// Every row with its raw identity fields, one per key (the first source
/// that reported it), each background item under the rows it is linked to,
/// sorted by name (then key).
fn rows(reports: &[InventoryReport]) -> Vec<(SoftwareItemView, Raw)> {
    let mut items: Vec<(SoftwareItemView, Raw)> = Vec::new();
    for report in reports {
        let rows = report
            .applications
            .iter()
            .filter_map(application)
            .chain(report.packages.iter().map(package));
        for row in rows {
            if !items.iter().any(|(item, _)| item.key == row.0.key) {
                items.push(row);
            }
        }
    }
    let apps: Vec<&Application> = reports.iter().flat_map(|r| &r.applications).collect();
    let services: Vec<&Service> = reports.iter().flat_map(|r| &r.services).collect();
    for link in link_services(&apps, &services) {
        let Some(key) = application(apps[link.application]).map(|(row, _)| row.key) else {
            continue;
        };
        let service = services[link.service];
        let view = linked(service, link.method, link.verified(), link.ambiguous);
        if let Some((item, raw)) = items.iter_mut().find(|(item, _)| item.key == key)
            && !item.services.contains(&view)
        {
            item.services.push(view);
            raw.services.push(SoftwareService::new(
                service.kind,
                service.label.raw(),
                link.verified(),
            ));
        }
    }
    for (item, _) in &mut items {
        item.services
            .sort_by_key(|s| (!s.verified, s.label.clone()));
    }
    items.sort_by_cached_key(|(item, _)| {
        (
            item.name.as_deref().unwrap_or(&item.id).to_lowercase(),
            item.key.clone(),
        )
    });
    items
}

impl SoftwareView {
    /// The list from every inventory report, sorted by name (then identifier),
    /// one row per key: a record two sources share is shown once, from the
    /// first source that reported it. Each background item is listed under
    /// the rows it is linked to.
    #[must_use]
    pub fn from_reports(reports: &[InventoryReport]) -> Self {
        let sources = reports
            .iter()
            .map(|report| {
                let rows = report.applications.iter().filter_map(application).count()
                    + report.packages.len();
                SoftwareSourceView {
                    source: report.source.as_str().to_owned(),
                    state: state(report.status),
                    count: u32::try_from(rows + report.services.len()).unwrap_or(u32::MAX),
                }
            })
            .collect();
        let items = rows(reports).into_iter().map(|(item, _)| item).collect();
        Self { items, sources }
    }

    /// The list with each row's cached AI note, when one may be shown
    /// (ADR-0061).
    #[must_use]
    pub fn with_notes(mut self, notes: &NoteCache) -> Self {
        for item in &mut self.items {
            item.ai_note = notes.shown(&software_key(item)).map(SoftwareNoteView::from);
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use lumen_domain::{AppId, RawPath, SourceName, UninstallRecord, UntrustedText};

    use super::*;

    fn report(source: &str, status: SourceStatus) -> InventoryReport {
        InventoryReport {
            source: SourceName::new(source).unwrap_or_else(|_| unreachable!()),
            status,
            applications: Vec::new(),
            services: Vec::new(),
            packages: Vec::new(),
        }
    }

    fn app(scheme: AppIdScheme, id: &str, name: &str, location: &str) -> Application {
        Application {
            ids: BTreeSet::from([AppId {
                scheme,
                value: UntrustedText::new(id),
            }]),
            name: Some(UntrustedText::new(name)),
            version: Some(UntrustedText::new("1.0")),
            vendor: None,
            signature: None,
            install_locations: vec![
                RawPath::from_unix_bytes(location.as_bytes().to_vec())
                    .unwrap_or_else(|_| unreachable!()),
            ],
            sources: BTreeSet::new(),
            uninstall: None,
        }
    }

    fn package(manager: PackageManager, id: &str, required: bool) -> Package {
        Package {
            manager,
            id: UntrustedText::new(id),
            version: Some(UntrustedText::new("2.0")),
            install_root: None,
            required,
            explicit: Some(true),
            relations: Vec::new(),
        }
    }

    #[test]
    fn rows_come_from_every_report_sorted_by_name() {
        let mut apps = report("macos.applications", SourceStatus::Complete);
        apps.applications = vec![
            app(
                AppIdScheme::BundleId,
                "com.example.Zed",
                "Zed",
                "/Applications/Zed.app",
            ),
            app(
                AppIdScheme::BundleId,
                "com.apple.calculator",
                "Calculator",
                "/System/Applications/Calculator.app",
            ),
        ];
        let mut brew = report("macos.homebrew", SourceStatus::Partial);
        brew.packages = vec![
            package(PackageManager::Homebrew, "act", false),
            package(PackageManager::HomebrewCask, "docker", false),
            package(PackageManager::Homebrew, "docker", false),
        ];
        let view = SoftwareView::from_reports(&[
            apps,
            brew,
            report("macos.package-receipts", SourceStatus::Unavailable),
        ]);
        let rows: Vec<_> = view
            .items
            .iter()
            .map(|i| (i.origin, i.id.as_str(), i.protected))
            .collect();
        assert_eq!(
            rows,
            [
                (SoftwareOriginView::Homebrew, "act", false),
                (SoftwareOriginView::MacApp, "com.apple.calculator", true),
                (SoftwareOriginView::Homebrew, "docker", false),
                (SoftwareOriginView::HomebrewCask, "docker", false),
                (SoftwareOriginView::MacApp, "com.example.Zed", false),
            ],
            "a formula and a cask with one name stay apart; system apps are protected"
        );
        let sources: Vec<_> = view
            .sources
            .iter()
            .map(|s| (s.source.as_str(), s.state, s.count))
            .collect();
        assert_eq!(
            sources,
            [
                ("macos.applications", SoftwareSourceStateView::Complete, 2),
                ("macos.homebrew", SoftwareSourceStateView::Partial, 3),
                (
                    "macos.package-receipts",
                    SoftwareSourceStateView::Unavailable,
                    0
                ),
            ]
        );
    }

    #[test]
    fn required_packages_and_unremovable_programs_are_protected() {
        let mut dpkg = report("linux.dpkg", SourceStatus::Complete);
        dpkg.packages = vec![package(PackageManager::Dpkg, "base-files:amd64", true)];
        let mut windows = report("windows.uninstall-keys", SourceStatus::Complete);
        let mut program = app(AppIdScheme::UninstallKey, "Tool", "Tool", "/x");
        program.ids.insert(AppId {
            scheme: AppIdScheme::MsiProductCode,
            value: UntrustedText::new("{11111111-2222-3333-4444-555555555555}"),
        });
        program.uninstall = Some(UninstallRecord {
            no_remove: true,
            hidden: true,
            ..UninstallRecord::default()
        });
        windows.applications = vec![program];
        let view = SoftwareView::from_reports(&[dpkg, windows]);
        assert!(view.items.iter().all(|i| i.protected));
        let program = view
            .items
            .iter()
            .find(|i| i.origin == SoftwareOriginView::WindowsProgram)
            .unwrap_or_else(|| unreachable!());
        assert_eq!(
            program.id, "{11111111-2222-3333-4444-555555555555}",
            "listed by its product code"
        );
        assert!(program.hidden);
    }

    fn agent(label: &str, program: Option<&str>) -> Service {
        Service {
            kind: ServiceKind::LaunchAgent,
            scope: ServiceScope::User,
            label: UntrustedText::new(label),
            definition: None,
            program: program.map(|p| {
                RawPath::from_unix_bytes(p.as_bytes().to_vec()).unwrap_or_else(|_| unreachable!())
            }),
            enabled: Some(true),
            signature: None,
        }
    }

    #[test]
    fn services_are_listed_under_the_software_they_are_linked_to() {
        let mut apps = report("macos.applications", SourceStatus::Complete);
        apps.applications = vec![app(
            AppIdScheme::BundleId,
            "com.example.Zed",
            "Zed",
            "/Applications/Zed.app",
        )];
        let mut launchd = report("macos.launchd", SourceStatus::Complete);
        launchd.services = vec![
            agent(
                "com.example.Zed.helper\u{202e}",
                Some("/Library/Zed/helper"),
            ),
            agent(
                "org.other.updater",
                Some("/Applications/Zed.app/Contents/MacOS/updater"),
            ),
            agent("org.unrelated", Some("/usr/local/bin/x")),
        ];
        let view = SoftwareView::from_reports(&[apps, launchd]);
        assert_eq!(view.items.len(), 1, "services are never rows");
        let linked: Vec<_> = view.items[0]
            .services
            .iter()
            .map(|s| (s.label.as_str(), s.method, s.verified, s.ambiguous))
            .collect();
        assert_eq!(
            linked,
            [
                (
                    "org.other.updater",
                    ServiceLinkMethodView::ProgramInsideApplication,
                    true,
                    false
                ),
                (
                    "com.example.Zed.helper⟦U+202E⟧",
                    ServiceLinkMethodView::LabelMatchesBundleId,
                    false,
                    false
                ),
            ]
        );
        assert_eq!(view.sources[1].count, 3, "background items are counted");
    }

    #[test]
    fn names_and_ids_are_display_escaped() {
        let mut snap = report("linux.snap", SourceStatus::Complete);
        snap.packages = vec![package(PackageManager::Snap, "evil\u{202e}txt", false)];
        let view = SoftwareView::from_reports(&[snap]);
        assert!(!view.items[0].id.contains('\u{202e}'));
    }

    #[test]
    fn requests_carry_identity_fields_only() -> Result<(), Box<dyn std::error::Error>> {
        let mut apps = report("macos.applications", SourceStatus::Complete);
        let mut zed = app(
            AppIdScheme::BundleId,
            "com.example.Zed",
            "Zed",
            "/Users/alice/Applications/Zed.app",
        );
        zed.vendor = Some(UntrustedText::new("Example\u{202e} Inc."));
        apps.applications = vec![zed];
        let mut launchd = report("macos.launchd", SourceStatus::Complete);
        launchd.services = (0..10)
            .map(|i| {
                agent(
                    &format!("com.example.Zed.helper{i}"),
                    Some(&format!("/Users/alice/Applications/Zed.app/Contents/h{i}")),
                )
            })
            .collect();
        let mut brew = report("macos.homebrew", SourceStatus::Complete);
        brew.packages = vec![package(PackageManager::Homebrew, "openssl@3", false)];
        brew.packages[0].explicit = Some(false);
        brew.packages[0].install_root = Some(RawPath::from_unix_bytes(
            b"/Users/alice/homebrew/Cellar/openssl@3".to_vec(),
        )?);
        let subjects = software_subjects(&[apps, launchd, brew]);
        let keys: Vec<_> = subjects
            .iter()
            .map(|s| (s.key.id.as_str(), s.key.version.as_deref(), s.preferred))
            .collect();
        assert_eq!(
            keys,
            [
                ("openssl@3", Some("2.0"), false),
                ("com.example.Zed", Some("1.0"), true)
            ],
            "the list's order; a dependency is not preferred"
        );
        let zed = serde_json::to_value(&subjects[1].request)?;
        assert_eq!(zed["services"].as_array().map(Vec::len), Some(8));
        assert_eq!(zed["services_truncated"], true);
        assert_eq!(zed["services"][0]["verified"], true);
        assert_eq!(
            zed["untrusted_strings"]["publisher"]["text"], "Example\u{202e} Inc.",
            "raw text, labelled untrusted"
        );
        for subject in &subjects {
            let text = serde_json::to_string(&subject.request)?;
            assert!(!text.contains("/Users"), "no path is sent: {text}");
            assert!(!text.contains("alice"), "no user name is sent: {text}");
        }
        Ok(())
    }

    #[test]
    fn only_notes_that_may_be_shown_are_listed() -> Result<(), Box<dyn std::error::Error>> {
        use lumen_domain::{JudgeDescriptor, ModelId};
        let mut brew = report("macos.homebrew", SourceStatus::Complete);
        brew.packages = vec![
            package(PackageManager::Homebrew, "node", false),
            package(PackageManager::Homebrew, "evil", false),
            package(PackageManager::Homebrew, "new", false),
        ];
        let view = SoftwareView::from_reports(&[brew]);
        assert!(view.items.iter().all(|i| i.ai_note.is_none()));
        let judge = JudgeDescriptor {
            provider: SourceName::new("fake")?,
            model: ModelId::new("fake-1")?,
            model_version: None,
            prompt_version: "jev-software/1".parse()?,
        };
        let note = |injection_suspected| SoftwareNote {
            purpose: SoftwarePurpose::RuntimeOrLibrary,
            removal_impact: RemovalImpact::OtherSoftwareDepends,
            confidence: JudgmentConfidence::High,
            injection_suspected,
        };
        let key = |id: &str| SoftwareKey {
            origin: SoftwareOrigin::Homebrew,
            id: id.to_owned(),
            version: Some("2.0".to_owned()),
        };
        let mut cache = NoteCache::default();
        cache.insert(key("node"), &judge, note(false));
        cache.insert(key("evil"), &judge, note(true));
        let view = view.with_notes(&cache);
        let notes: Vec<_> = view
            .items
            .iter()
            .map(|i| (i.id.as_str(), serde_json::to_value(i.ai_note).ok()))
            .collect();
        assert_eq!(
            notes,
            [
                ("evil", Some(serde_json::Value::Null)),
                ("new", Some(serde_json::Value::Null)),
                (
                    "node",
                    Some(serde_json::json!({
                        "purpose": "runtime_or_library",
                        "removal_impact": "other_software_depends",
                        "confidence": "high",
                    }))
                ),
            ]
        );
        Ok(())
    }
}
