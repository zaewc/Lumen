//! Lumen's own facts about removing each listed application and package
//! (roadmap 16.6, #850), from inventory records alone. Pure and IO-free.
//!
//! - **Mechanism.** A package is removed by its manager, except a macOS
//!   installer receipt, which records no uninstaller. An application is
//!   removed as its Windows uninstall entry says (Windows Installer, or a
//!   command of its own), or by the package that verifiably installed it
//!   ([`crate::software_relations`]).
//! - **Administrator rights.** Known to be needed only where the mechanism
//!   always needs them: dpkg, RPM, pacman and snap keep a system-wide
//!   database, and the receipts in `/var/db/receipts` are written by system
//!   installs. Anything else is not known, never "not needed".
//! - **Dependencies.** What a package's record says it needs (every other
//!   package it names, listed or not: the record's own statement), and which
//!   listed packages' records say they need it, counted only where the
//!   relation is verified; a set of alternatives counts as one need, and
//!   makes each candidate's dependents a lower bound. "None needs it" holds
//!   only where every inventory was read whole, every package's relations
//!   were read, and no dependency naming it is ambiguous (ADR-0008): a
//!   dependent may sit in any report.
//!
//! Protection is the list's to decide (it also looks at install locations), so
//! it is left `false` here. The model's answer is never an input.

use std::collections::BTreeSet;

use lumen_domain::{
    Application, Package, PackageId, PackageManager, PackageRelationKind, RelationCount,
    RemovalFacts, RemovalMechanism, SourceStatus,
};

use crate::ports::InventoryReport;
use crate::software_relations::{RelationTarget, ResolvedKind, resolve_relations};

/// The facts of every application and package in some inventory reports.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct InventoryFacts {
    /// One per application, in the order of
    /// `reports.iter().flat_map(|r| &r.applications)`.
    pub applications: Vec<RemovalFacts>,
    /// One per package, in the order of
    /// `reports.iter().flat_map(|r| &r.packages)`.
    pub packages: Vec<RemovalFacts>,
}

/// How a package's manager removes it, and whether that always needs
/// administrator rights.
const fn manager_facts(manager: PackageManager) -> (Option<RemovalMechanism>, bool) {
    match manager {
        PackageManager::MacInstaller => (Some(RemovalMechanism::NoneRecorded), true),
        PackageManager::Msi => (Some(RemovalMechanism::WindowsInstaller), false),
        PackageManager::Dpkg
        | PackageManager::Rpm
        | PackageManager::Pacman
        | PackageManager::Snap => (Some(RemovalMechanism::PackageManager), true),
        PackageManager::Homebrew
        | PackageManager::HomebrewCask
        | PackageManager::Msix
        | PackageManager::Winget
        | PackageManager::Android
        | PackageManager::Flatpak => (Some(RemovalMechanism::PackageManager), false),
        _ => (None, false),
    }
}

/// How an application's own Windows uninstall entry removes it.
fn recorded_mechanism(app: &Application) -> Option<RemovalMechanism> {
    let record = app.uninstall.as_ref()?;
    Some(if record.windows_installer {
        RemovalMechanism::WindowsInstaller
    } else if record.command.is_some() || record.quiet_command.is_some() {
        RemovalMechanism::OwnUninstaller
    } else {
        RemovalMechanism::NoneRecorded
    })
}

fn count(n: usize, complete: bool) -> RelationCount {
    RelationCount {
        count: u32::try_from(n).unwrap_or(u32::MAX),
        complete,
    }
}

/// Lumen's removal facts for every application and package in `reports`.
#[must_use]
pub fn inventory_facts(reports: &[InventoryReport]) -> InventoryFacts {
    let applications: Vec<&Application> = reports.iter().flat_map(|r| &r.applications).collect();
    let packages: Vec<&Package> = reports.iter().flat_map(|r| &r.packages).collect();
    // Whether every dependent is known: every report read whole, and every
    // package's relations read. A dependent can sit in any report.
    let all_read = reports.iter().all(|r| {
        r.status == SourceStatus::Complete && r.packages.iter().all(|p| p.relations.is_some())
    });
    let resolved = resolve_relations(&applications, &packages);

    let package_facts = packages
        .iter()
        .enumerate()
        .map(|(p, package)| {
            let (mechanism, needs_administrator) = manager_facts(package.manager);
            let needs = package.relations.as_ref().map(|relations| {
                let own = package.package_id();
                // One need per named package, and one per set of alternatives
                // it does not meet itself.
                let needed: BTreeSet<Vec<&PackageId>> = relations
                    .iter()
                    .filter_map(|r| match &r.kind {
                        PackageRelationKind::DependsOn {
                            package: needed, ..
                        } => Some(vec![needed]),
                        PackageRelationKind::DependsOnOneOf { packages, .. } => {
                            let mut set: Vec<_> = packages.iter().collect();
                            set.sort_unstable();
                            set.dedup();
                            Some(set)
                        }
                        _ => None,
                    })
                    .filter(|needed| !needed.contains(&&own))
                    .collect();
                count(needed.len(), true)
            });
            let dependents: BTreeSet<usize> = resolved
                .iter()
                .filter(|r| r.verified() && r.target == RelationTarget::Package(p))
                .filter(|r| matches!(r.kind, ResolvedKind::DependsOn { .. }))
                .map(|r| r.package)
                .filter(|&d| packages[d].package_id() != package.package_id())
                .collect();
            // A dependency that names this package but also another listed
            // entry (ambiguous) may be on this one: the count is then at
            // least, never all.
            let unsure = resolved.iter().any(|r| {
                !r.verified()
                    && r.target == RelationTarget::Package(p)
                    && matches!(r.kind, ResolvedKind::DependsOn { .. })
                    && packages[r.package].package_id() != package.package_id()
            });
            RemovalFacts {
                protected: false,
                mechanism,
                installed_by: None,
                needs_administrator,
                needs,
                needed_by: Some(count(dependents.len(), all_read && !unsure)),
            }
        })
        .collect();

    let application_facts = applications
        .iter()
        .enumerate()
        .map(|(a, app)| {
            // The packages that verifiably installed it; only one is an owner.
            let installers: BTreeSet<usize> = resolved
                .iter()
                .filter(|r| r.verified() && r.kind == ResolvedKind::Installed)
                .filter(|r| r.target == RelationTarget::Application(a))
                .map(|r| r.package)
                .collect();
            let installer = match installers.iter().collect::<Vec<_>>().as_slice() {
                [only] => Some(packages[**only].manager),
                _ => None,
            };
            let (mechanism, installed_by, needs_administrator) =
                match (recorded_mechanism(app), installer) {
                    (Some(mechanism), _) => (Some(mechanism), None, false),
                    (None, Some(manager)) => {
                        let (mechanism, administrator) = manager_facts(manager);
                        (mechanism, Some(manager), administrator)
                    }
                    (None, None) => (None, None, false),
                };
            RemovalFacts {
                protected: false,
                mechanism,
                installed_by,
                needs_administrator,
                needs: None,
                needed_by: None,
            }
        })
        .collect();

    InventoryFacts {
        applications: application_facts,
        packages: package_facts,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use lumen_domain::{
        AppId, AppIdScheme, PackageId, PackageRelation, RawPath, RelationMethod, SourceName,
        UninstallRecord, UntrustedText,
    };

    use super::*;

    fn report(
        status: SourceStatus,
        applications: Vec<Application>,
        packages: Vec<Package>,
    ) -> InventoryReport {
        InventoryReport {
            source: SourceName::new("test.inventory").unwrap_or_else(|_| unreachable!()),
            status,
            applications,
            services: Vec::new(),
            packages,
        }
    }

    fn app(id: &str, location: &str, uninstall: Option<UninstallRecord>) -> Application {
        Application {
            ids: BTreeSet::from([AppId {
                scheme: AppIdScheme::BundleId,
                value: UntrustedText::new(id),
            }]),
            name: None,
            version: None,
            vendor: None,
            signature: None,
            install_locations: vec![
                RawPath::from_unix_bytes(location.as_bytes().to_vec())
                    .unwrap_or_else(|_| unreachable!()),
            ],
            sources: BTreeSet::new(),
            uninstall,
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

    fn depends(manager: PackageManager, id: &str) -> PackageRelation {
        PackageRelation {
            kind: PackageRelationKind::DependsOn {
                package: PackageId {
                    manager,
                    id: UntrustedText::new(id),
                },
                direct: Some(true),
            },
            method: RelationMethod::HomebrewRuntimeDependencies,
        }
    }

    fn installed(location: &str, method: RelationMethod) -> PackageRelation {
        PackageRelation {
            kind: PackageRelationKind::Installed {
                location: RawPath::from_unix_bytes(location.as_bytes().to_vec())
                    .unwrap_or_else(|_| unreachable!()),
            },
            method,
        }
    }

    const BREW: PackageManager = PackageManager::Homebrew;

    #[test]
    fn dependencies_count_both_ways_and_none_only_when_all_was_read() {
        let packages = vec![
            package(BREW, "openssl@3", Some(Vec::new())),
            package(BREW, "python@3.13", Some(vec![depends(BREW, "openssl@3")])),
            package(
                BREW,
                "curl",
                Some(vec![depends(BREW, "openssl@3"), depends(BREW, "gone")]),
            ),
            // A cask of the same name is another package.
            package(
                PackageManager::HomebrewCask,
                "wget",
                Some(vec![depends(BREW, "curl")]),
            ),
        ];
        let facts =
            inventory_facts(&[report(SourceStatus::Complete, Vec::new(), packages.clone())]);
        let by = |i: usize| facts.packages[i].needed_by.map(|c| (c.count, c.complete));
        assert_eq!(
            by(0),
            Some((2, true)),
            "openssl is needed by python and curl"
        );
        assert_eq!(
            by(1),
            Some((0, true)),
            "nothing needs python, and all was read"
        );
        assert_eq!(by(2), Some((1, true)));
        assert_eq!(
            facts.packages[2].needs.map(|c| c.count),
            Some(2),
            "an unlisted need still counts"
        );
        assert!(facts.packages[0].relied_on());
        assert!(!facts.packages[1].relied_on());

        // A partial inventory, or one package's relations unread: none is unknown.
        let partial =
            inventory_facts(&[report(SourceStatus::Partial, Vec::new(), packages.clone())]);
        assert_eq!(
            partial.packages[1].needed_by.map(|c| c.complete),
            Some(false)
        );
        let mut unread = packages;
        unread[3].relations = None;
        let unread = inventory_facts(&[report(SourceStatus::Complete, Vec::new(), unread)]);
        assert_eq!(
            unread.packages[2].needed_by.map(|c| (c.count, c.complete)),
            Some((0, false))
        );
        assert_eq!(
            unread.packages[3].needs, None,
            "an unread record needs unknown"
        );

        // A dependent can sit in another report: one unread there leaves
        // every count incomplete.
        let lone = vec![package(BREW, "lone", Some(Vec::new()))];
        let casks = vec![package(PackageManager::HomebrewCask, "x", None)];
        let elsewhere = inventory_facts(&[
            report(SourceStatus::Complete, Vec::new(), lone),
            report(SourceStatus::Complete, Vec::new(), casks),
        ]);
        assert_eq!(
            elsewhere.packages[0]
                .needed_by
                .map(|c| (c.count, c.complete)),
            Some((0, false))
        );
    }

    #[test]
    fn an_ambiguous_or_self_dependency_is_not_a_dependent() {
        let packages = vec![
            package(BREW, "lib", Some(Vec::new())),
            package(BREW, "lib", Some(Vec::new())),
            package(
                BREW,
                "tool",
                Some(vec![depends(BREW, "lib"), depends(BREW, "tool")]),
            ),
        ];
        let facts = inventory_facts(&[report(SourceStatus::Complete, Vec::new(), packages)]);
        assert_eq!(
            facts.packages[0].needed_by.map(|c| c.count),
            Some(0),
            "two `lib`s: ambiguous"
        );
        assert_eq!(
            facts.packages[0].needed_by.map(|c| c.complete),
            Some(false),
            "`tool` may need either `lib`: never \"nothing needs it\""
        );
        assert_eq!(
            facts.packages[2].needed_by.map(|c| c.count),
            Some(0),
            "itself is not another"
        );
        assert_eq!(
            facts.packages[2].needs.map(|c| c.count),
            Some(1),
            "the record names `lib` once, and itself does not count"
        );
    }

    #[test]
    fn alternatives_are_one_need_and_leave_each_candidates_dependents_open() {
        const DPKG: PackageManager = PackageManager::Dpkg;
        let one_of = |names: &[&str]| PackageRelation {
            kind: PackageRelationKind::DependsOnOneOf {
                packages: names
                    .iter()
                    .map(|n| PackageId {
                        manager: DPKG,
                        id: UntrustedText::new(*n),
                    })
                    .collect(),
                direct: Some(true),
            },
            method: RelationMethod::HomebrewRuntimeDependencies,
        };
        let packages = vec![
            package(DPKG, "postfix", Some(Vec::new())),
            package(DPKG, "exim4", Some(Vec::new())),
            package(
                DPKG,
                "mutt",
                Some(vec![
                    one_of(&["postfix", "exim4"]),
                    one_of(&["exim4", "postfix"][..1]),
                    depends(DPKG, "libc6"),
                ]),
            ),
            package(
                DPKG,
                "self-mta",
                Some(vec![one_of(&["self-mta", "postfix"])]),
            ),
        ];
        let facts = inventory_facts(&[report(SourceStatus::Complete, Vec::new(), packages)]);
        let needed_by = |i: usize| facts.packages[i].needed_by.map(|c| (c.count, c.complete));
        assert_eq!(needed_by(0), Some((0, false)), "`mutt` may need postfix");
        assert_eq!(needed_by(1), Some((0, false)), "or exim4");
        assert_eq!(
            facts.packages[2].needs.map(|c| c.count),
            Some(3),
            "two sets of alternatives and one package"
        );
        assert_eq!(
            facts.packages[3].needs.map(|c| c.count),
            Some(0),
            "a set it meets itself is no need"
        );
    }

    #[test]
    fn the_mechanism_and_administrator_rights_come_from_records() {
        let packages = vec![
            package(PackageManager::Dpkg, "curl", None),
            package(PackageManager::MacInstaller, "com.example.pkg", None),
            package(BREW, "jq", None),
        ];
        let facts = inventory_facts(&[report(SourceStatus::Complete, Vec::new(), packages)]);
        let of = |i: usize| {
            (
                facts.packages[i].mechanism,
                facts.packages[i].needs_administrator,
            )
        };
        assert_eq!(of(0), (Some(RemovalMechanism::PackageManager), true));
        assert_eq!(of(1), (Some(RemovalMechanism::NoneRecorded), true));
        assert_eq!(
            of(2),
            (Some(RemovalMechanism::PackageManager), false),
            "not known, never not needed"
        );

        let entry = |windows_installer, command: Option<&str>| UninstallRecord {
            windows_installer,
            command: command.map(UntrustedText::new),
            ..UninstallRecord::default()
        };
        let apps = vec![
            app(
                "msi",
                "/Program Files/A",
                Some(entry(true, Some("msiexec /x {A}"))),
            ),
            app(
                "own",
                "/Program Files/B",
                Some(entry(false, Some("uninstall.exe"))),
            ),
            app("none", "/Program Files/C", Some(entry(false, None))),
            app("unrecorded", "/Applications/D.app", None),
        ];
        let facts = inventory_facts(&[report(SourceStatus::Complete, apps, Vec::new())]);
        let mechanisms: Vec<_> = facts.applications.iter().map(|f| f.mechanism).collect();
        assert_eq!(
            mechanisms,
            [
                Some(RemovalMechanism::WindowsInstaller),
                Some(RemovalMechanism::OwnUninstaller),
                Some(RemovalMechanism::NoneRecorded),
                None,
            ]
        );
        assert!(
            facts
                .applications
                .iter()
                .all(|f| f.needed_by.is_none() && f.needs.is_none())
        );
    }

    #[test]
    fn only_a_verified_installer_gives_an_app_its_mechanism() {
        let apps = vec![
            app("org.example.cask", "/Applications/Cask.app", None),
            app("org.example.receipt", "/Applications/Receipt.app", None),
            app("org.example.guess", "/Applications/Guess.app", None),
            app("org.example.contested", "/Applications/Contested.app", None),
        ];
        let packages = vec![
            package(
                PackageManager::HomebrewCask,
                "cask",
                Some(vec![
                    installed(
                        "/Applications/Cask.app",
                        RelationMethod::CaskUninstallArtifacts,
                    ),
                    installed(
                        "/Applications/Contested.app",
                        RelationMethod::CaskUninstallArtifacts,
                    ),
                ]),
            ),
            package(
                PackageManager::MacInstaller,
                "com.example.receipt",
                Some(vec![
                    installed("/Applications/Receipt.app", RelationMethod::ReceiptFileList),
                    installed(
                        "/Applications/Contested.app",
                        RelationMethod::ReceiptFileList,
                    ),
                ]),
            ),
            package(
                PackageManager::HomebrewCask,
                "guess",
                Some(vec![installed(
                    "/Applications/Guess.app",
                    RelationMethod::CaskDefaultAppFolder,
                )]),
            ),
        ];
        let facts = inventory_facts(&[report(SourceStatus::Complete, apps, packages)]);
        let of = |i: usize| {
            (
                facts.applications[i].mechanism,
                facts.applications[i].needs_administrator,
            )
        };
        assert_eq!(
            of(0),
            (Some(RemovalMechanism::PackageManager), false),
            "Homebrew installed it"
        );
        assert_eq!(
            of(1),
            (Some(RemovalMechanism::NoneRecorded), true),
            "a system receipt"
        );
        assert_eq!(of(2), (None, false), "an inferred location proves nothing");
        assert_eq!(of(3), (None, false), "two unrelated owners: no owner");
        let by: Vec<_> = facts.applications.iter().map(|f| f.installed_by).collect();
        assert_eq!(
            by,
            [
                Some(PackageManager::HomebrewCask),
                Some(PackageManager::MacInstaller),
                None,
                None
            ],
            "the source of the mechanism"
        );
    }
}
