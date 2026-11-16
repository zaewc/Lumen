//! What each row of the software list is related to (ADR-0064): what a
//! package needs and installed, read from its manager's own records, and on
//! the other side what needs it and what installed it. Resolved purely by
//! [`lumen_application::software_relations`]; display only, nothing here acts
//! on a relation.

use lumen_application::software_relations::{RelationTarget, ResolvedKind, resolve_relations};
use lumen_domain::{Application, Package, PackageRelationKind, RelationMethod};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// What a relation states, from the row's side.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SoftwareRelationKindView {
    /// This package's record says it needs the other to run.
    Needs,
    /// The other package's record says it needs this one.
    NeededBy,
    /// This package's record says it installed the other.
    Installed,
    /// The other package's record says it installed this one.
    InstalledBy,
}

/// The record a relation was read from, or the default it assumed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum SoftwareRelationMethodView {
    /// Homebrew's install receipt (`runtime_dependencies`).
    HomebrewRuntimeDependencies,
    /// A cask's install receipt (`uninstall_artifacts`).
    CaskUninstallArtifacts,
    /// The cask definition Homebrew stored at install time.
    CaskDefinition,
    /// A cask app assumed to be in `/Applications`: a default, never proof.
    CaskDefaultAppFolder,
    /// An installer receipt's file list records the bundle's
    /// `Contents/Info.plist`.
    ReceiptFileList,
    /// Another record.
    Other,
}

/// One relation of a row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct SoftwareRelationView {
    /// What it states, from this row's side.
    pub kind: SoftwareRelationKindView,
    /// The other row's key, or `None` where no row shows the other. That
    /// means "not installed" only where the inventory that would list it was
    /// read whole (see `sources`).
    #[schema(required = true)]
    pub other: Option<String>,
    /// The other side as recorded: a package identifier, or an installed
    /// location; display-escaped.
    pub label: String,
    /// The record it was read from.
    pub method: SoftwareRelationMethodView,
    /// For a dependency, whether the needing package declares it itself
    /// rather than through another dependency; `None` where not recorded or
    /// not a dependency.
    #[schema(required = true)]
    pub direct: Option<bool>,
    /// Proven: recorded, matched to one row, and claimed by no unrelated
    /// package.
    pub verified: bool,
    /// It matched several rows, or another package claims the same app: one
    /// candidate, not a fact.
    pub ambiguous: bool,
}

const fn method(method: RelationMethod) -> SoftwareRelationMethodView {
    match method {
        RelationMethod::HomebrewRuntimeDependencies => {
            SoftwareRelationMethodView::HomebrewRuntimeDependencies
        }
        RelationMethod::CaskUninstallArtifacts => {
            SoftwareRelationMethodView::CaskUninstallArtifacts
        }
        RelationMethod::CaskDefinition => SoftwareRelationMethodView::CaskDefinition,
        RelationMethod::CaskDefaultAppFolder => SoftwareRelationMethodView::CaskDefaultAppFolder,
        RelationMethod::ReceiptFileList => SoftwareRelationMethodView::ReceiptFileList,
        _ => SoftwareRelationMethodView::Other,
    }
}

/// How the other side of a relation reads when it is not in the list.
fn recorded_label(kind: &PackageRelationKind) -> Option<String> {
    match kind {
        PackageRelationKind::DependsOn { package, .. }
        | PackageRelationKind::InstalledPackage { package } => Some(package.id.display()),
        PackageRelationKind::Installed { location } => Some(location.display()),
        _ => None,
    }
}

/// Every relation of `packages`, as `(row key, view)` pairs for both sides:
/// the package whose record states it, and the listed row it points at.
/// `app_keys` and `package_keys` hold the list's key of each application and
/// package, by index (`None` for an application the list does not show).
pub(super) fn relation_views(
    applications: &[&Application],
    packages: &[&Package],
    app_keys: &[Option<String>],
    package_keys: &[String],
) -> Vec<(String, SoftwareRelationView)> {
    let mut views = Vec::new();
    for resolved in resolve_relations(applications, packages) {
        let package = packages[resolved.package];
        let Some(relation) = package
            .relations
            .as_ref()
            .and_then(|relations| relations.get(resolved.relation))
        else {
            continue;
        };
        let Some(recorded) = recorded_label(&relation.kind) else {
            continue;
        };
        let (forward, backward, direct) = match resolved.kind {
            ResolvedKind::DependsOn { direct } => (
                SoftwareRelationKindView::Needs,
                SoftwareRelationKindView::NeededBy,
                direct,
            ),
            ResolvedKind::Installed => (
                SoftwareRelationKindView::Installed,
                SoftwareRelationKindView::InstalledBy,
                None,
            ),
        };
        let other = match resolved.target {
            RelationTarget::Package(p) => package_keys.get(p).cloned(),
            RelationTarget::Application(a) => app_keys.get(a).cloned().flatten(),
            RelationTarget::Unlisted => None,
        };
        let Some(own) = package_keys.get(resolved.package).cloned() else {
            continue;
        };
        // Proven only where the other side is a row: an application the list
        // does not show is matched to no row.
        let verified = resolved.verified() && other.is_some();
        let view = |kind, other, label| SoftwareRelationView {
            kind,
            other,
            label,
            method: method(resolved.method),
            direct,
            verified,
            ambiguous: resolved.ambiguous,
        };
        if let Some(other) = &other {
            views.push((
                other.clone(),
                view(backward, Some(own.clone()), package.id.display()),
            ));
        }
        views.push((own, view(forward, other, recorded)));
    }
    views
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use lumen_application::ports::InventoryReport;
    use lumen_domain::{
        AppId, AppIdScheme, PackageId, PackageManager, PackageRelation, RawPath, SourceName,
        SourceStatus, UntrustedText,
    };

    use super::super::{SoftwareItemView, SoftwareView};
    use super::*;

    fn report(source: &str) -> InventoryReport {
        InventoryReport {
            source: SourceName::new(source).unwrap_or_else(|_| unreachable!()),
            status: SourceStatus::Complete,
            applications: Vec::new(),
            services: Vec::new(),
            packages: Vec::new(),
        }
    }

    fn path(text: &str) -> RawPath {
        RawPath::from_unix_bytes(text.as_bytes().to_vec()).unwrap_or_else(|_| unreachable!())
    }

    fn app(id: &str, location: &str) -> Application {
        Application {
            ids: BTreeSet::from([AppId {
                scheme: AppIdScheme::BundleId,
                value: UntrustedText::new(id),
            }]),
            name: Some(UntrustedText::new(id)),
            version: None,
            vendor: None,
            signature: None,
            install_locations: vec![path(location)],
            sources: BTreeSet::new(),
            uninstall: None,
        }
    }

    fn package(
        manager: PackageManager,
        id: &str,
        relations: Option<Vec<PackageRelation>>,
    ) -> Package {
        Package {
            manager,
            id: UntrustedText::new(id),
            version: None,
            install_root: None,
            required: false,
            explicit: Some(true),
            relations,
        }
    }

    fn needs(id: &str, direct: bool) -> PackageRelation {
        PackageRelation {
            kind: PackageRelationKind::DependsOn {
                package: PackageId {
                    manager: PackageManager::Homebrew,
                    id: UntrustedText::new(id),
                },
                direct: Some(direct),
            },
            method: RelationMethod::HomebrewRuntimeDependencies,
        }
    }

    fn installed(location: &str, method: RelationMethod) -> PackageRelation {
        PackageRelation {
            kind: PackageRelationKind::Installed {
                location: path(location),
            },
            method,
        }
    }

    fn row<'a>(view: &'a SoftwareView, key: &str) -> &'a SoftwareItemView {
        view.items
            .iter()
            .find(|item| item.key == key)
            .unwrap_or_else(|| unreachable!("no row {key}"))
    }

    type Brief<'a> = (
        SoftwareRelationKindView,
        Option<&'a str>,
        &'a str,
        bool,
        bool,
    );

    fn brief(item: &SoftwareItemView) -> Vec<Brief<'_>> {
        item.relations
            .iter()
            .map(|r| {
                (
                    r.kind,
                    r.other.as_deref(),
                    r.label.as_str(),
                    r.verified,
                    r.ambiguous,
                )
            })
            .collect()
    }

    #[test]
    fn dependencies_show_on_both_sides_and_unlisted_ones_stay_unmatched() {
        use SoftwareRelationKindView::{NeededBy, Needs};
        let mut brew = report("macos.homebrew");
        brew.packages = vec![
            package(
                PackageManager::Homebrew,
                "ffmpeg",
                Some(vec![needs("x264", true), needs("gone", false)]),
            ),
            package(PackageManager::Homebrew, "x264", Some(Vec::new())),
            package(PackageManager::Homebrew, "unread", None),
        ];
        let view = SoftwareView::from_reports(&[brew]);
        assert_eq!(
            brief(row(&view, "Homebrew:ffmpeg")),
            [
                (Needs, Some("Homebrew:x264"), "x264", true, false),
                (Needs, None, "gone", false, false),
            ]
        );
        assert_eq!(
            row(&view, "Homebrew:ffmpeg").relations[0].direct,
            Some(true)
        );
        assert_eq!(
            brief(row(&view, "Homebrew:x264")),
            [(NeededBy, Some("Homebrew:ffmpeg"), "ffmpeg", true, false)]
        );
        assert_eq!(row(&view, "Homebrew:x264").relations_read, Some(true));
        assert_eq!(
            row(&view, "Homebrew:unread").relations_read,
            Some(false),
            "an unread record is unknown, never none"
        );
    }

    #[test]
    fn a_package_two_sources_report_is_one_target() {
        let mut first = report("macos.homebrew");
        first.packages = vec![
            package(
                PackageManager::Homebrew,
                "ffmpeg",
                Some(vec![needs("x264", true)]),
            ),
            package(PackageManager::Homebrew, "x264", Some(Vec::new())),
        ];
        let mut second = report("macos.homebrew.other");
        second.packages = vec![package(PackageManager::Homebrew, "x264", Some(Vec::new()))];
        let view = SoftwareView::from_reports(&[first, second]);
        assert_eq!(
            brief(row(&view, "Homebrew:ffmpeg")),
            [(
                SoftwareRelationKindView::Needs,
                Some("Homebrew:x264"),
                "x264",
                true,
                false
            )]
        );
    }

    #[test]
    fn an_app_the_list_does_not_show_is_never_verified() {
        let mut apps = report("android.packages");
        let mut hidden = app("com.example.Zed", "/Applications/Zed.app");
        hidden.ids = BTreeSet::from([AppId {
            scheme: AppIdScheme::AndroidPackage,
            value: UntrustedText::new("com.example.zed"),
        }]);
        apps.applications = vec![hidden];
        let mut receipts = report("macos.receipts");
        receipts.packages = vec![package(
            PackageManager::MacInstaller,
            "com.example.zed.pkg",
            Some(vec![installed(
                "/Applications/Zed.app",
                RelationMethod::ReceiptFileList,
            )]),
        )];
        let view = SoftwareView::from_reports(&[apps, receipts]);
        assert_eq!(view.items.len(), 1, "the app has no row");
        assert_eq!(
            brief(&view.items[0]),
            [(
                SoftwareRelationKindView::Installed,
                None,
                "/Applications/Zed.app",
                false,
                false
            )]
        );
    }

    #[test]
    fn an_app_shows_the_package_that_installed_it() {
        use SoftwareRelationKindView::{Installed, InstalledBy};
        let mut apps = report("macos.applications");
        apps.applications = vec![app("org.mozilla.firefox", "/Applications/Firefox.app")];
        let mut brew = report("macos.homebrew");
        brew.packages = vec![package(
            PackageManager::HomebrewCask,
            "firefox",
            Some(vec![installed(
                "/Applications/Firefox.app",
                RelationMethod::CaskUninstallArtifacts,
            )]),
        )];
        let view = SoftwareView::from_reports(&[apps, brew]);
        let firefox = row(&view, "MacApp:org.mozilla.firefox");
        assert_eq!(
            brief(firefox),
            [(
                InstalledBy,
                Some("HomebrewCask:firefox"),
                "firefox",
                true,
                false
            )]
        );
        assert_eq!(
            firefox.relations_read, None,
            "apps have no record of their own"
        );
        assert_eq!(
            brief(row(&view, "HomebrewCask:firefox")),
            [(
                Installed,
                Some("MacApp:org.mozilla.firefox"),
                "/Applications/Firefox.app",
                true,
                false
            )]
        );
    }

    #[test]
    fn contested_and_inferred_ownership_is_never_verified() {
        let mut apps = report("macos.applications");
        apps.applications = vec![
            app("com.example.Zed", "/Applications/Zed.app"),
            app("com.example.Yak", "/Applications/Yak.app"),
        ];
        let mut receipts = report("macos.receipts");
        receipts.packages = vec![
            package(
                PackageManager::MacInstaller,
                "com.example.zed.pkg",
                Some(vec![installed(
                    "/Applications/Zed.app",
                    RelationMethod::ReceiptFileList,
                )]),
            ),
            package(
                PackageManager::MacInstaller,
                "com.other.bundle.pkg",
                Some(vec![installed(
                    "/Applications/Zed.app",
                    RelationMethod::ReceiptFileList,
                )]),
            ),
        ];
        let mut brew = report("macos.homebrew");
        brew.packages = vec![package(
            PackageManager::HomebrewCask,
            "yak",
            Some(vec![installed(
                "/Applications/Yak.app",
                RelationMethod::CaskDefaultAppFolder,
            )]),
        )];
        let view = SoftwareView::from_reports(&[apps, receipts, brew]);
        let zed = row(&view, "MacApp:com.example.Zed");
        assert_eq!(zed.relations.len(), 2);
        assert!(zed.relations.iter().all(|r| r.ambiguous && !r.verified));
        let yak = &row(&view, "MacApp:com.example.Yak").relations;
        assert_eq!(yak.len(), 1);
        assert_eq!(
            yak[0].method,
            SoftwareRelationMethodView::CaskDefaultAppFolder
        );
        assert!(!yak[0].verified, "a default folder is never proof");
    }
}
