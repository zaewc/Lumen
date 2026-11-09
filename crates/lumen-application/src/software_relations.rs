//! What installed packages are related to, resolved against the rest of the
//! inventory (ADR-0064): which listed package a recorded dependency names,
//! which listed application a package installed, and where ownership is
//! ambiguous. Pure and IO-free.
//!
//! - A dependency or an installed package resolves by its [`PackageId`]
//!   (manager and identifier), never by a name alone: a Homebrew formula and
//!   a cask with the same name are different packages.
//! - An installed location resolves to the application whose recorded install
//!   location is that same location, compared component by component as in
//!   [`crate::software_links`], so `/Applications/Foo.app.old` is not
//!   `/Applications/Foo.app`. A location fewer than two components below its
//!   root (`/Applications`) is too broad to name an application.
//! - When packages that did not install one another each record installing
//!   the same application, none of them is its owner: every such claim is
//!   marked ambiguous and none is verified. A location or identifier that
//!   matches several entries is ambiguous too.
//! - A relation whose target is in no inventory given is
//!   [`RelationTarget::Unlisted`]. That is "not installed" only where the
//!   inventory that would list it was read whole (ADR-0008), which the caller
//!   knows from each report's status.
//! - Only a recorded relation can be verified; an inferred one never is.
//! - A package whose relations are unknown (`None`) contributes nothing, so
//!   "nothing depends on this" holds only where every package of the source
//!   was read (`Some`) and the source itself was read whole; the caller
//!   decides that from the reports.

use lumen_domain::{
    Application, Package, PackageId, PackageRelationKind, RelationBasis, RelationMethod,
};

use crate::software_links::{Components, components};

/// What a relation points at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RelationTarget {
    /// Index into the packages given.
    Package(usize),
    /// Index into the applications given.
    Application(usize),
    /// In no inventory given.
    Unlisted,
}

/// What the relation states.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ResolvedKind {
    /// The package needs the target to run.
    DependsOn {
        /// Declared by the package itself, not through another dependency;
        /// `None` where the record does not say.
        direct: Option<bool>,
    },
    /// The package installed the target.
    Installed,
}

/// One relation of one package, resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ResolvedRelation {
    /// Index into the packages given: the package whose record states it.
    pub package: usize,
    /// Index into that package's [`relations`](Package::relations).
    pub relation: usize,
    /// What it states.
    pub kind: ResolvedKind,
    /// The record it was read from.
    pub method: RelationMethod,
    /// What it points at.
    pub target: RelationTarget,
    /// It matched several entries, or another package claims the same
    /// application: this is one candidate, not a fact.
    pub ambiguous: bool,
}

impl ResolvedRelation {
    /// Whether the relation is proven: recorded, resolved to one listed
    /// entry, and unambiguous.
    pub const fn verified(&self) -> bool {
        matches!(self.method.basis(), RelationBasis::Recorded)
            && !self.ambiguous
            && !matches!(self.target, RelationTarget::Unlisted)
    }
}

/// Resolves every relation of every package, ordered by package and then by
/// relation. A relation that matches several entries gives one result per
/// match, each ambiguous.
pub fn resolve_relations(
    applications: &[&Application],
    packages: &[&Package],
) -> Vec<ResolvedRelation> {
    let locations: Vec<Vec<Components<'_>>> = applications
        .iter()
        .map(|app| {
            app.install_locations
                .iter()
                .filter_map(components)
                .filter(|c| c.depth() >= 2)
                .collect()
        })
        .collect();
    let named = |id: &PackageId| -> Vec<RelationTarget> {
        (0..packages.len())
            .filter(|&p| packages[p].manager == id.manager && packages[p].id == id.id)
            .map(RelationTarget::Package)
            .collect()
    };
    let mut resolved = Vec::new();
    for (p, package) in packages.iter().enumerate() {
        for (r, relation) in package.relations.iter().flatten().enumerate() {
            let (kind, matches, ambiguous) = match &relation.kind {
                PackageRelationKind::DependsOn { package, direct } => {
                    let matches = named(package);
                    let ambiguous = matches.len() > 1;
                    (
                        ResolvedKind::DependsOn { direct: *direct },
                        matches,
                        ambiguous,
                    )
                }
                PackageRelationKind::InstalledPackage { package } => {
                    let matches = named(package);
                    let ambiguous = matches.len() > 1;
                    (ResolvedKind::Installed, matches, ambiguous)
                }
                PackageRelationKind::Installed { location } => {
                    let found: Vec<usize> = components(location)
                        .filter(|c| c.depth() >= 2)
                        .map_or_else(Vec::new, |location| {
                            (0..applications.len())
                                .filter(|&a| locations[a].iter().any(|l| l.is_same(&location)))
                                .collect()
                        });
                    // Records that share an identifier are one application.
                    let ambiguous = found.iter().enumerate().any(|(i, &a)| {
                        found[i + 1..]
                            .iter()
                            .any(|&b| applications[a].ids.is_disjoint(&applications[b].ids))
                    });
                    let matches = found.into_iter().map(RelationTarget::Application).collect();
                    (ResolvedKind::Installed, matches, ambiguous)
                }
                // A kind this resolver does not know yet resolves to nothing.
                _ => (ResolvedKind::Installed, Vec::new(), false),
            };
            let one = |target| ResolvedRelation {
                package: p,
                relation: r,
                kind,
                method: relation.method,
                target,
                ambiguous,
            };
            if matches.is_empty() {
                resolved.push(one(RelationTarget::Unlisted));
            } else {
                resolved.extend(matches.into_iter().map(one));
            }
        }
    }
    mark_conflicting_owners(&mut resolved, packages);
    resolved
}

/// Marks as ambiguous every claim on an application that some other package,
/// unrelated by installation, also claims.
fn mark_conflicting_owners(resolved: &mut [ResolvedRelation], packages: &[&Package]) {
    let installed = |a: usize, b: usize| {
        resolved.iter().any(|r| {
            r.package == a
                && r.kind == ResolvedKind::Installed
                && r.target == RelationTarget::Package(b)
        })
    };
    let claims = |app: usize| -> Vec<usize> {
        resolved
            .iter()
            .filter(|r| r.kind == ResolvedKind::Installed)
            .filter(|r| r.target == RelationTarget::Application(app))
            .map(|r| r.package)
            .collect()
    };
    let contested: Vec<usize> = resolved
        .iter()
        .filter_map(|r| match r.target {
            RelationTarget::Application(app) => Some(app),
            RelationTarget::Package(_) | RelationTarget::Unlisted => None,
        })
        .filter(|&app| {
            let owners = claims(app);
            owners.iter().any(|&a| {
                owners.iter().any(|&b| {
                    packages[a].package_id() != packages[b].package_id()
                        && !installed(a, b)
                        && !installed(b, a)
                })
            })
        })
        .collect();
    for r in resolved.iter_mut() {
        if let RelationTarget::Application(app) = r.target
            && contested.contains(&app)
        {
            r.ambiguous = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use lumen_domain::{
        AppId, AppIdScheme, PackageManager, PackageRelation, RawPath, UntrustedText,
    };

    use super::*;

    fn unix(path: &str) -> RawPath {
        RawPath::from_unix_bytes(path.as_bytes().to_vec()).unwrap_or_else(|_| unreachable!())
    }

    fn app(id: &str, location: &str) -> Application {
        Application {
            ids: BTreeSet::from([AppId {
                scheme: AppIdScheme::BundleId,
                value: UntrustedText::new(id),
            }]),
            name: None,
            version: None,
            vendor: None,
            signature: None,
            install_locations: vec![unix(location)],
            sources: BTreeSet::new(),
            uninstall: None,
        }
    }

    fn id(manager: PackageManager, id: &str) -> PackageId {
        PackageId {
            manager,
            id: UntrustedText::new(id),
        }
    }

    fn package(manager: PackageManager, name: &str, relations: Vec<PackageRelation>) -> Package {
        Package {
            manager,
            id: UntrustedText::new(name),
            version: None,
            install_root: None,
            required: false,
            explicit: None,
            relations: Some(relations),
        }
    }

    fn depends(manager: PackageManager, name: &str) -> PackageRelation {
        PackageRelation {
            kind: PackageRelationKind::DependsOn {
                package: id(manager, name),
                direct: Some(true),
            },
            method: RelationMethod::HomebrewRuntimeDependencies,
        }
    }

    fn installed(location: &str, method: RelationMethod) -> PackageRelation {
        PackageRelation {
            kind: PackageRelationKind::Installed {
                location: unix(location),
            },
            method,
        }
    }

    fn installed_package(name: &str) -> PackageRelation {
        PackageRelation {
            kind: PackageRelationKind::InstalledPackage {
                package: id(PackageManager::MacInstaller, name),
            },
            method: RelationMethod::CaskUninstallArtifacts,
        }
    }

    /// `(package, target, verified)` for each resolved relation.
    fn resolve(apps: &[Application], packages: &[Package]) -> Vec<(usize, RelationTarget, bool)> {
        let apps: Vec<&Application> = apps.iter().collect();
        let packages: Vec<&Package> = packages.iter().collect();
        resolve_relations(&apps, &packages)
            .into_iter()
            .map(|r| (r.package, r.target, r.verified()))
            .collect()
    }

    const RECEIPT: RelationMethod = RelationMethod::ReceiptFileList;
    use RelationTarget::{Application as App, Package as Pkg, Unlisted};

    #[test]
    fn a_dependency_resolves_by_manager_and_identifier_only() {
        let packages = [
            package(
                PackageManager::Homebrew,
                "ffmpeg",
                vec![depends(PackageManager::Homebrew, "docker")],
            ),
            package(PackageManager::HomebrewCask, "docker", vec![]),
            package(PackageManager::Homebrew, "docker", vec![]),
        ];
        assert_eq!(resolve(&[], &packages), [(0, Pkg(2), true)]);
    }

    #[test]
    fn a_dependency_that_is_not_listed_is_unlisted_and_unverified() {
        let packages = [package(
            PackageManager::Homebrew,
            "ffmpeg",
            vec![depends(PackageManager::Homebrew, "x264")],
        )];
        assert_eq!(resolve(&[], &packages), [(0, Unlisted, false)]);
    }

    #[test]
    fn a_recorded_bundle_is_owned_and_a_look_alike_is_not() {
        let apps = [
            app("com.example.Foo", "/Applications/Foo.app"),
            app("com.example.Old", "/Applications/Foo.app.old"),
        ];
        let packages = [package(
            PackageManager::MacInstaller,
            "com.example.pkg",
            vec![installed("/Applications/Foo.app/", RECEIPT)],
        )];
        assert_eq!(resolve(&apps, &packages), [(0, App(0), true)]);
    }

    #[test]
    fn two_unrelated_packages_claiming_one_app_own_it_ambiguously() {
        let apps = [app("com.example.Foo", "/Applications/Foo.app")];
        let packages = [
            package(
                PackageManager::MacInstaller,
                "com.example.pkg",
                vec![installed("/Applications/Foo.app", RECEIPT)],
            ),
            package(
                PackageManager::MacInstaller,
                "com.example.pkg.old",
                vec![installed("/Applications/Foo.app", RECEIPT)],
            ),
        ];
        assert_eq!(
            resolve(&apps, &packages),
            [(0, App(0), false), (1, App(0), false)]
        );
    }

    #[test]
    fn a_cask_that_ran_the_receipts_installer_does_not_contest_it() {
        let apps = [app("us.zoom.xos", "/Applications/zoom.us.app")];
        let packages = [
            package(
                PackageManager::HomebrewCask,
                "zoom",
                vec![
                    installed_package("us.zoom.pkg.videomeeting"),
                    installed("/Applications/zoom.us.app", RelationMethod::CaskDefinition),
                ],
            ),
            package(
                PackageManager::MacInstaller,
                "us.zoom.pkg.videomeeting",
                vec![installed("/Applications/zoom.us.app", RECEIPT)],
            ),
        ];
        assert_eq!(
            resolve(&apps, &packages),
            [(0, Pkg(1), true), (0, App(0), true), (1, App(0), true)]
        );
    }

    #[test]
    fn an_inferred_location_is_never_verified() {
        let apps = [app("com.xs-labs.Hot", "/Applications/Hot.app")];
        let packages = [package(
            PackageManager::HomebrewCask,
            "hot",
            vec![installed(
                "/Applications/Hot.app",
                RelationMethod::CaskDefaultAppFolder,
            )],
        )];
        assert_eq!(resolve(&apps, &packages), [(0, App(0), false)]);
    }

    #[test]
    fn a_broad_location_names_no_application() {
        let apps = [app("com.example.Foo", "/Applications")];
        let packages = [package(
            PackageManager::MacInstaller,
            "com.example.pkg",
            vec![installed("/Applications", RECEIPT)],
        )];
        assert_eq!(resolve(&apps, &packages), [(0, Unlisted, false)]);
    }

    #[test]
    fn a_location_two_applications_report_is_ambiguous() {
        let apps = [
            app("com.example.Foo", "/Applications/Foo.app"),
            app("com.example.Other", "/Applications/Foo.app"),
        ];
        let packages = [package(
            PackageManager::MacInstaller,
            "com.example.pkg",
            vec![installed("/Applications/Foo.app", RECEIPT)],
        )];
        assert_eq!(
            resolve(&apps, &packages),
            [(0, App(0), false), (0, App(1), false)]
        );
    }
}
