//! The installed software list (ADR-0060 step 3): one row per application or
//! package that a platform inventory reports, with where the record came from
//! and how completely each inventory was read. Read only; nothing here removes
//! anything.

use lumen_application::ports::InventoryReport;
use lumen_domain::{AppIdScheme, Application, Package, PackageManager, SourceStatus};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

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
    /// How many rows it gave.
    pub count: u32,
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

fn package(package: &Package) -> SoftwareItemView {
    let origin = origin(package.manager);
    let id = package.id.display();
    SoftwareItemView {
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
        id,
    }
}

/// The identifier an application is listed by: its product code for a
/// Windows Installer program, else the first identifier it has.
fn application(app: &Application) -> Option<SoftwareItemView> {
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
    .find_map(|(scheme, origin)| pick(scheme).map(|id| (origin, id.value.display())))?;
    let location = app.install_locations.first();
    // macOS's own applications live in /System/Applications (and the Safari
    // cryptex), which the system volume's protection keeps read-only.
    let system_app = location.is_some_and(|l| {
        let bytes = l.as_bytes();
        bytes.starts_with(b"/System/") || bytes.starts_with(b"/Library/Apple/")
    });
    let record = app.uninstall.as_ref();
    Some(SoftwareItemView {
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
        id,
    })
}

impl SoftwareView {
    /// The list from every inventory report, sorted by name (then identifier),
    /// one row per key: a record two sources share is shown once, from the
    /// first source that reported it.
    #[must_use]
    pub fn from_reports(reports: &[InventoryReport]) -> Self {
        let mut items: Vec<SoftwareItemView> = Vec::new();
        let mut sources = Vec::new();
        for report in reports {
            let rows: Vec<SoftwareItemView> = report
                .applications
                .iter()
                .filter_map(application)
                .chain(report.packages.iter().map(package))
                .collect();
            sources.push(SoftwareSourceView {
                source: report.source.as_str().to_owned(),
                state: state(report.status),
                count: u32::try_from(rows.len()).unwrap_or(u32::MAX),
            });
            for row in rows {
                if !items.iter().any(|item| item.key == row.key) {
                    items.push(row);
                }
            }
        }
        items.sort_by_cached_key(|item| {
            (
                item.name.as_deref().unwrap_or(&item.id).to_lowercase(),
                item.key.clone(),
            )
        });
        Self { items, sources }
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

    #[test]
    fn names_and_ids_are_display_escaped() {
        let mut snap = report("linux.snap", SourceStatus::Complete);
        snap.packages = vec![package(PackageManager::Snap, "evil\u{202e}txt", false)];
        let view = SoftwareView::from_reports(&[snap]);
        assert!(!view.items[0].id.contains('\u{202e}'));
    }
}
