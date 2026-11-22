//! A Linux package manager's own record of what each package needs, turned
//! into relations (ADR-0064). Shared by the sources that read whole databases.
//!
//! A need names a package, or several alternatives any one of which meets it.
//! A name is met by the installed package of that name or by any installed
//! package that provides it ([Debian Policy 7.5]; pacman and RPM work alike).
//! Only a need with one alternative, met by exactly one package of that name
//! and provided by none, names one package; every other need lists all its
//! candidates and stays ambiguous: Lumen never picks the one that "really"
//! meets it. A name nothing installed meets is kept as recorded (unlisted),
//! and an unversioned need the package meets itself is left out only where
//! the caller can prove names suffice to meet its needs.
//!
//! [Debian Policy 7.5]: https://www.debian.org/doc/debian-policy/ch-relationships.html#virtual-packages-provides

use std::collections::HashMap;

use lumen_domain::{
    Package, PackageId, PackageRelation, PackageRelationKind, RelationMethod, UntrustedText,
};

/// One installed package as its record states it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Declared {
    /// The name other packages' needs use.
    pub name: String,
    /// Whether names alone can prove this package meets its own needs.
    pub allow_self_satisfaction: bool,
    /// The names it provides.
    pub provides: Vec<String>,
    /// Each need, as its alternatives' names; `None` where the record could
    /// not be read.
    pub needs: Option<Vec<Vec<String>>>,
}

/// Sets each package's relations from its record: `declared[i]` describes
/// `packages[i]`. Both are every installed package of one manager, read whole.
pub(crate) fn link(packages: &mut [Package], declared: &[Declared], method: RelationMethod) {
    let mut by_name: HashMap<&str, Vec<usize>> = HashMap::new();
    let mut providers: HashMap<&str, Vec<usize>> = HashMap::new();
    for (i, d) in declared.iter().enumerate() {
        by_name.entry(&d.name).or_default().push(i);
        for name in &d.provides {
            providers.entry(name).or_default().push(i);
        }
    }
    let ids: Vec<UntrustedText> = packages.iter().map(|p| p.id.clone()).collect();
    for (own, (package, d)) in packages.iter_mut().zip(declared).enumerate() {
        let manager = package.manager;
        let by_index = |i: &usize| PackageId {
            manager,
            id: ids[*i].clone(),
        };
        package.relations = d.needs.as_ref().map(|needs| {
            needs
                .iter()
                .filter_map(|alternatives| {
                    // Each alternative's packages by name, then its providers.
                    let found: Vec<(&str, Vec<usize>, bool)> = alternatives
                        .iter()
                        .map(|name| {
                            let name = name.as_str();
                            let named = by_name.get(name).cloned().unwrap_or_default();
                            let provided = providers.get(name).cloned().unwrap_or_default();
                            let only_by_name = provided.is_empty();
                            (name, [named, provided].concat(), only_by_name)
                        })
                        .collect();
                    // A need the package meets itself is no need of another.
                    if d.allow_self_satisfaction
                        && found
                            .iter()
                            .any(|(_, candidates, _)| candidates.contains(&own))
                    {
                        return None;
                    }
                    let kind = match found.as_slice() {
                        [] => return None,
                        [(_, candidates, true)] if candidates.len() == 1 => {
                            PackageRelationKind::DependsOn {
                                package: by_index(&candidates[0]),
                                direct: Some(true),
                            }
                        }
                        [(name, candidates, _)] if candidates.is_empty() => {
                            PackageRelationKind::DependsOn {
                                package: PackageId {
                                    manager,
                                    id: UntrustedText::new(*name),
                                },
                                direct: Some(true),
                            }
                        }
                        _ => {
                            let mut packages: Vec<PackageId> = Vec::new();
                            for (name, candidates, _) in &found {
                                if candidates.is_empty() {
                                    packages.push(PackageId {
                                        manager,
                                        id: UntrustedText::new(*name),
                                    });
                                }
                                for candidate in candidates.iter().map(by_index) {
                                    if !packages.contains(&candidate) {
                                        packages.push(candidate);
                                    }
                                }
                            }
                            PackageRelationKind::DependsOnOneOf {
                                packages,
                                direct: Some(true),
                            }
                        }
                    };
                    Some(PackageRelation { kind, method })
                })
                .collect()
        });
    }
}

#[cfg(test)]
mod tests {
    use lumen_domain::PackageManager;

    use super::*;

    fn package(id: &str) -> Package {
        Package {
            manager: PackageManager::Dpkg,
            id: UntrustedText::new(id),
            version: None,
            install_root: None,
            required: false,
            explicit: None,
            relations: None,
        }
    }

    fn owned(names: &[&str]) -> Vec<String> {
        names.iter().map(|n| (*n).to_owned()).collect()
    }

    fn declared(name: &str, provides: &[&str], needs: Option<&[&[&str]]>) -> Declared {
        Declared {
            name: name.to_owned(),
            allow_self_satisfaction: true,
            provides: owned(provides),
            needs: needs.map(|n| n.iter().map(|a| owned(a)).collect()),
        }
    }

    /// Each relation as `(one, ids)`: `one` for a single named package.
    fn brief(package: &Package) -> Option<Vec<(bool, Vec<String>)>> {
        let raw = |p: &PackageId| p.id.raw().to_owned();
        package.relations.as_ref().map(|relations| {
            relations
                .iter()
                .map(|r| match &r.kind {
                    PackageRelationKind::DependsOn { package, direct } => {
                        assert_eq!(*direct, Some(true));
                        (true, vec![raw(package)])
                    }
                    PackageRelationKind::DependsOnOneOf { packages, direct } => {
                        assert_eq!(*direct, Some(true));
                        (false, packages.iter().map(raw).collect())
                    }
                    other => unreachable!("{other:?}"),
                })
                .collect()
        })
    }

    #[test]
    fn only_a_need_one_package_meets_by_name_names_it() {
        let mut packages = [
            "mutt:amd64",
            "postfix:amd64",
            "exim4-daemon-light:amd64",
            "libc6:amd64",
            "libc6:i386",
            "unread:amd64",
        ]
        .map(package);
        let declared = [
            declared(
                "mutt",
                &[],
                Some(&[
                    &["postfix"],
                    &["default-mta", "mail-transport-agent"],
                    &["libc6"],
                    &["gone"],
                    &["gone", "postfix"],
                ]),
            ),
            declared("postfix", &["mail-transport-agent"], Some(&[])),
            declared("exim4-daemon-light", &["mail-transport-agent"], Some(&[])),
            declared("libc6", &[], Some(&[])),
            declared("libc6", &[], Some(&[])),
            declared("unread", &[], None),
        ];
        link(&mut packages, &declared, RelationMethod::DpkgStatus);
        assert_eq!(
            brief(&packages[0]),
            Some(vec![
                (true, owned(&["postfix:amd64"])),
                // A virtual name: every provider, never one of them.
                (
                    false,
                    owned(&["default-mta", "postfix:amd64", "exim4-daemon-light:amd64"])
                ),
                // Two architectures installed: either may meet it.
                (false, owned(&["libc6:amd64", "libc6:i386"])),
                (true, owned(&["gone"])),
                (false, owned(&["gone", "postfix:amd64"])),
            ])
        );
        assert_eq!(brief(&packages[1]), Some(Vec::new()));
        assert_eq!(brief(&packages[5]), None, "unread is unknown, never none");
    }

    #[test]
    fn a_need_the_package_meets_itself_is_left_out() {
        let mut packages = ["libfoo:amd64", "other:amd64"].map(package);
        let declared = [
            declared(
                "libfoo",
                &["libfoo.so.1"],
                Some(&[&["libfoo.so.1"], &["other", "libfoo.so.1"], &["other"]]),
            ),
            declared("other", &[], Some(&[])),
        ];
        link(&mut packages, &declared, RelationMethod::DpkgStatus);
        assert_eq!(
            brief(&packages[0]),
            Some(vec![(true, owned(&["other:amd64"]))])
        );
    }

    #[test]
    fn a_name_only_a_provider_meets_stays_ambiguous() {
        let mut packages = ["needs-sh:amd64", "dash:amd64"].map(package);
        let declared = [
            declared("needs-sh", &[], Some(&[&["sh"]])),
            declared("dash", &["sh"], Some(&[])),
        ];
        link(&mut packages, &declared, RelationMethod::DpkgStatus);
        assert_eq!(
            brief(&packages[0]),
            Some(vec![(false, vec!["dash:amd64".to_owned()])])
        );
    }
}
