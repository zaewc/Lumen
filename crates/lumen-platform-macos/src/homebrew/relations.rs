//! What Homebrew's own records say a formula or cask needs and installed
//! (ADR-0064), read from the JSON it writes; `brew` is never run.
//!
//! - A formula's install receipt lists every runtime dependency, each with
//!   whether the formula declares it directly. Homebrew reads the same field
//!   back for an installed keg.
//! - A cask's install receipt (Homebrew 5 and later) lists its runtime
//!   dependencies, formulae and casks, and its `uninstall_artifacts`: the
//!   `app` bundles it placed and the installer packages its `uninstall
//!   pkgutil:` removes. An older cask keeps only the definition Homebrew stored
//!   when installing it, whose `artifacts` have the same shape.
//! - An `app` lands in the `appdir` of the cask's recorded configuration. If
//!   none is recorded, `/Applications` is assumed, and the relation says it was
//!   inferred. A target under `~` is skipped, and so is a `pkgutil` value that
//!   is a pattern rather than one identifier: Lumen does not guess what it
//!   matches.

use lumen_domain::{
    PackageId, PackageManager, PackageRelation, PackageRelationKind, RawPath, RelationMethod,
    UntrustedText,
};
use serde_json::Value;

/// A formula's relations from its install receipt; `None` if the receipt does
/// not list its dependencies.
pub(super) fn formula(receipt: &Value) -> Option<Vec<PackageRelation>> {
    let dependencies = receipt.get("runtime_dependencies")?.as_array()?;
    Some(depends_on(dependencies, PackageManager::Homebrew))
}

/// A cask's relations: from its install receipt when that lists its
/// uninstall artifacts, else from its stored definition's `artifacts`;
/// `None` if neither is readable.
pub(super) fn cask(
    receipt: Option<&Value>,
    definition: Option<&Value>,
    config: Option<&Value>,
) -> Option<Vec<PackageRelation>> {
    let (artifacts, method) = match receipt.and_then(|r| r.get("uninstall_artifacts")) {
        Some(artifacts) => (artifacts, RelationMethod::CaskUninstallArtifacts),
        None => (
            definition?.get("artifacts")?,
            RelationMethod::CaskDefinition,
        ),
    };
    let mut relations = Vec::new();
    if let Some(dependencies) = receipt.and_then(|r| r.get("runtime_dependencies")) {
        for (key, manager) in [
            ("formula", PackageManager::Homebrew),
            ("cask", PackageManager::HomebrewCask),
        ] {
            if let Some(list) = dependencies.get(key).and_then(Value::as_array) {
                relations.extend(depends_on(list, manager));
            }
        }
    }
    let appdir = config.and_then(appdir);
    for artifact in artifacts.as_array().into_iter().flatten() {
        if let Some(app) = artifact.get("app").and_then(Value::as_array) {
            relations.extend(app_location(app, appdir, method));
        }
        let uninstall = artifact.get("uninstall").and_then(Value::as_array);
        for directive in uninstall.into_iter().flatten() {
            relations.extend(pkgutil(directive.get("pkgutil"), method));
        }
    }
    Some(relations)
}

/// `runtime_dependencies` entries as dependencies on packages of `manager`.
/// A tap's `full_name` (`user/tap/name`) is installed under its last part.
fn depends_on(list: &[Value], manager: PackageManager) -> Vec<PackageRelation> {
    list.iter()
        .filter_map(|dependency| {
            let full_name = dependency.get("full_name")?.as_str()?;
            let name = full_name.rsplit('/').next().filter(|n| !n.is_empty())?;
            Some(PackageRelation {
                kind: PackageRelationKind::DependsOn {
                    package: PackageId {
                        manager,
                        id: UntrustedText::new(name),
                    },
                    direct: dependency.get("declared_directly").and_then(Value::as_bool),
                },
                method: RelationMethod::HomebrewRuntimeDependencies,
            })
        })
        .collect()
}

/// The configured `appdir`, as Homebrew resolves it: explicit, then from the
/// environment, then the default.
fn appdir(config: &Value) -> Option<&str> {
    ["explicit", "env", "default"]
        .into_iter()
        .find_map(|layer| config.get(layer)?.get("appdir")?.as_str())
        .filter(|dir| dir.starts_with('/'))
}

/// Where an `app` artifact (`[source]` or `[source, {"target": …}]`) placed
/// its bundle.
fn app_location(
    app: &[Value],
    appdir: Option<&str>,
    method: RelationMethod,
) -> Option<PackageRelation> {
    let source = app.first()?.as_str()?;
    let target = app
        .get(1)
        .and_then(|options| options.get("target"))
        .and_then(Value::as_str);
    let (path, method) = match target {
        Some(target) if target.starts_with('~') => return None,
        Some(target) if target.starts_with('/') => (target.to_owned(), method),
        _ => {
            let name = target.or_else(|| source.rsplit('/').next())?;
            if name.is_empty() {
                return None;
            }
            match appdir {
                Some(dir) => (format!("{}/{name}", dir.trim_end_matches('/')), method),
                None => (
                    format!("/Applications/{name}"),
                    RelationMethod::CaskDefaultAppFolder,
                ),
            }
        }
    };
    Some(PackageRelation {
        kind: PackageRelationKind::Installed {
            location: RawPath::from_unix_bytes(path.into_bytes()).ok()?,
        },
        method,
    })
}

/// The installer packages an `uninstall pkgutil:` value names, one or a list.
/// Homebrew reads it as a regular expression; only a value that can match
/// nothing but itself is taken.
fn pkgutil(value: Option<&Value>, method: RelationMethod) -> Vec<PackageRelation> {
    let values: Vec<&str> = match value {
        Some(Value::String(one)) => vec![one],
        Some(Value::Array(many)) => many.iter().filter_map(Value::as_str).collect(),
        _ => Vec::new(),
    };
    values
        .into_iter()
        .filter(|id| {
            !id.is_empty()
                && id
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
        })
        .map(|id| PackageRelation {
            kind: PackageRelationKind::InstalledPackage {
                package: PackageId {
                    manager: PackageManager::MacInstaller,
                    id: UntrustedText::new(id),
                },
            },
            method,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn dependency(relation: &PackageRelation) -> Option<(PackageManager, &str, Option<bool>)> {
        match &relation.kind {
            PackageRelationKind::DependsOn { package, direct } => {
                Some((package.manager, package.id.raw(), *direct))
            }
            _ => None,
        }
    }

    fn location(relation: &PackageRelation) -> Option<(&[u8], RelationMethod)> {
        match &relation.kind {
            PackageRelationKind::Installed { location } => {
                Some((location.as_bytes(), relation.method))
            }
            _ => None,
        }
    }

    fn installed_package(relation: &PackageRelation) -> Option<&str> {
        match &relation.kind {
            PackageRelationKind::InstalledPackage { package } => Some(package.id.raw()),
            _ => None,
        }
    }

    #[test]
    fn a_formulas_dependencies_keep_whether_they_are_direct() {
        let receipt = json!({"runtime_dependencies": [
            {"full_name": "libsamplerate", "declared_directly": true},
            {"full_name": "libogg", "declared_directly": false},
            {"full_name": "someone/tap/thing"},
            {"version": "1.0"},
        ]});
        let relations = formula(&receipt).unwrap_or_default();
        let found: Vec<_> = relations.iter().filter_map(dependency).collect();
        assert_eq!(
            found,
            [
                (PackageManager::Homebrew, "libsamplerate", Some(true)),
                (PackageManager::Homebrew, "libogg", Some(false)),
                (PackageManager::Homebrew, "thing", None),
            ]
        );
        assert!(
            relations
                .iter()
                .all(|r| r.method == RelationMethod::HomebrewRuntimeDependencies)
        );
    }

    #[test]
    fn a_receipt_without_dependencies_leaves_them_unknown() {
        assert_eq!(formula(&json!({"installed_on_request": true})), None);
        assert_eq!(
            formula(&json!({"runtime_dependencies": []})),
            Some(Vec::new())
        );
    }

    #[test]
    fn a_casks_receipt_gives_its_app_packages_and_dependencies() {
        let receipt = json!({
            "runtime_dependencies": {
                "formula": [{"full_name": "openssl@3", "declared_directly": true}],
                "cask": [{"full_name": "temurin", "declared_directly": false}],
            },
            "uninstall_artifacts": [
                {"app": ["Hot.app"]},
                {"app": ["Folder/Tool.app", {"target": "Renamed.app"}]},
                {"app": ["Home.app", {"target": "~/Applications/Home.app"}]},
                {"uninstall": [
                    {"pkgutil": "com.microsoft.powershell", "launchctl": "x"},
                    {"pkgutil": ["com.example.one", "com.adobe.*"]},
                ]},
                {"zap": [{"trash": ["~/Library/Caches/com.xs-labs.Hot"]}]},
            ],
        });
        let config = json!({"default": {"appdir": "/Applications"}, "explicit": {"appdir": "/Users/me/Apps/"}});
        let relations = cask(Some(&receipt), None, Some(&config)).unwrap_or_default();
        let found: Vec<_> = relations.iter().filter_map(dependency).collect();
        assert_eq!(
            found,
            [
                (PackageManager::Homebrew, "openssl@3", Some(true)),
                (PackageManager::HomebrewCask, "temurin", Some(false)),
            ]
        );
        let apps: Vec<_> = relations.iter().filter_map(location).collect();
        assert_eq!(
            apps,
            [
                (
                    &b"/Users/me/Apps/Hot.app"[..],
                    RelationMethod::CaskUninstallArtifacts
                ),
                (
                    &b"/Users/me/Apps/Renamed.app"[..],
                    RelationMethod::CaskUninstallArtifacts
                ),
            ]
        );
        let packages: Vec<_> = relations.iter().filter_map(installed_package).collect();
        assert_eq!(packages, ["com.microsoft.powershell", "com.example.one"]);
    }

    #[test]
    fn an_older_cask_falls_back_to_its_definition_and_an_unknown_folder_is_inferred() {
        let definition = json!({"artifacts": [{"app": ["Old.app"]}]});
        let relations = cask(None, Some(&definition), None).unwrap_or_default();
        let apps: Vec<_> = relations.iter().filter_map(location).collect();
        assert_eq!(
            apps,
            [(
                &b"/Applications/Old.app"[..],
                RelationMethod::CaskDefaultAppFolder
            )]
        );
        let recorded = cask(
            None,
            Some(&definition),
            Some(&json!({"default": {"appdir": "/Applications"}})),
        )
        .unwrap_or_default();
        assert_eq!(recorded[0].method, RelationMethod::CaskDefinition);
        assert_eq!(cask(None, Some(&json!({"url_specs": {}})), None), None);
        assert_eq!(cask(None, None, None), None);
    }
}
