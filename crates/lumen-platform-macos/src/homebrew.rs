//! Homebrew formulae and casks (#806, ADR-0060), read from Homebrew's own
//! folders: `Cellar/<formula>/<version>` and `Caskroom/<cask>/<version>`,
//! with each install receipt's `installed_on_request`, and what each one's
//! records say it needs and installed ([`relations`], ADR-0064). Nothing is
//! run, so the list does not depend on `brew` being on a `PATH`.
//!
//! Both default prefixes are read: `/opt/homebrew` (Apple silicon) and
//! `/usr/local` (Intel). A Homebrew installed elsewhere is not found, and
//! neither prefix having Homebrew is [`SourceStatus::Unavailable`], never "no
//! packages".

use std::collections::BTreeMap;
use std::io::{ErrorKind, Read as _};
use std::path::{Path, PathBuf};

use lumen_application::CancelToken;
use lumen_application::ports::{InventoryReport, InventorySource};
use lumen_domain::{Package, PackageManager, RawPath, SourceName, SourceStatus, UntrustedText};

mod relations;

const SOURCE: &str = "macos.homebrew";

/// The largest install receipt read.
const MAX_RECEIPT: u64 = 1024 * 1024;

/// Homebrew's formulae and casks under its prefixes.
#[derive(Debug, Clone)]
pub struct HomebrewInventory {
    prefixes: Vec<PathBuf>,
}

impl HomebrewInventory {
    /// The default prefixes, `/opt/homebrew` and `/usr/local`.
    #[must_use]
    pub fn new() -> Self {
        Self::in_prefixes(vec![
            PathBuf::from("/opt/homebrew"),
            PathBuf::from("/usr/local"),
        ])
    }

    /// Other prefixes, for tests.
    #[must_use]
    pub const fn in_prefixes(prefixes: Vec<PathBuf>) -> Self {
        Self { prefixes }
    }
}

impl Default for HomebrewInventory {
    fn default() -> Self {
        Self::new()
    }
}

fn source() -> SourceName {
    SourceName::new(SOURCE).unwrap_or_else(|_| unreachable!("a valid literal"))
}

/// A JSON file Homebrew wrote (an install receipt, a cask's configuration or
/// stored definition); `None` if it is missing, unreadable, too large or not
/// JSON.
fn read_json(path: &Path) -> Option<serde_json::Value> {
    let mut text = String::new();
    std::fs::File::open(path)
        .ok()?
        .take(MAX_RECEIPT + 1)
        .read_to_string(&mut text)
        .ok()?;
    if text.len() as u64 > MAX_RECEIPT {
        return None;
    }
    serde_json::from_str(&text).ok()
}

/// `installed_on_request` from an install receipt, if it says.
fn on_request(receipt: &serde_json::Value) -> Option<bool> {
    receipt.get("installed_on_request")?.as_bool()
}

/// The cask definition Homebrew stored when it installed `version` of cask
/// `name` in `dir`: the newest `.metadata/<version>/<time>/Casks/<name>.json`.
fn stored_definition(dir: &Path, name: &str, version: &str) -> Option<serde_json::Value> {
    let installs = dir.join(".metadata").join(version);
    let newest = subdirectories(&installs)?.pop()?;
    read_json(
        &installs
            .join(newest)
            .join("Casks")
            .join(format!("{name}.json")),
    )
}

/// The visible subdirectory names of `dir`, sorted; `None` if it cannot be
/// listed.
fn subdirectories(dir: &Path) -> Option<Vec<String>> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .ok()?
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|t| t.is_dir()))
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| !name.starts_with('.'))
        .collect();
    names.sort();
    Some(names)
}

fn raw(path: &Path) -> Option<RawPath> {
    use std::os::unix::ffi::OsStrExt as _;
    RawPath::from_unix_bytes(path.as_os_str().as_bytes().to_vec()).ok()
}

/// One installed formula or cask: its folder `dir` holds a folder per
/// installed version; the receipts say whether it was asked for and what it
/// needs and installed (ADR-0064).
fn package(manager: PackageManager, name: &str, dir: &Path) -> Option<Package> {
    let versions = subdirectories(dir)?;
    // Several versions of a formula can be installed side by side; the last
    // in name order is shown, and any receipt saying "on request" counts.
    let version = versions.last()?;
    let (explicit, relations) = if manager == PackageManager::HomebrewCask {
        let metadata = dir.join(".metadata");
        let receipt = read_json(&metadata.join("INSTALL_RECEIPT.json"));
        let lists_artifacts = receipt
            .as_ref()
            .is_some_and(|r| r.get("uninstall_artifacts").is_some());
        let definition = if lists_artifacts {
            None
        } else {
            stored_definition(dir, name, version)
        };
        let config = read_json(&metadata.join("config.json"));
        (
            receipt.as_ref().and_then(on_request),
            relations::cask(receipt.as_ref(), definition.as_ref(), config.as_ref()),
        )
    } else {
        let receipts: Vec<Option<serde_json::Value>> = versions
            .iter()
            .map(|v| read_json(&dir.join(v).join("INSTALL_RECEIPT.json")))
            .collect();
        let explicit = receipts
            .iter()
            .flatten()
            .filter_map(on_request)
            .reduce(|a, b| a || b);
        // What the shown version needs.
        let relations = receipts
            .last()
            .and_then(Option::as_ref)
            .and_then(relations::formula);
        (explicit, relations)
    };
    Some(Package {
        manager,
        id: UntrustedText::new(name),
        version: Some(UntrustedText::new(version.clone())),
        install_root: raw(dir),
        // Homebrew has no "part of the base system" mark.
        required: false,
        explicit,
        relations,
    })
}

impl InventorySource for HomebrewInventory {
    fn name(&self) -> SourceName {
        source()
    }

    fn collect(&self, cancel: &CancelToken) -> InventoryReport {
        let mut found = BTreeMap::new();
        let mut any = false;
        let mut complete = true;
        'prefixes: for prefix in &self.prefixes {
            for (folder, manager) in [
                ("Cellar", PackageManager::Homebrew),
                ("Caskroom", PackageManager::HomebrewCask),
            ] {
                let dir = prefix.join(folder);
                let names = match std::fs::read_dir(&dir) {
                    Ok(_) => subdirectories(&dir),
                    Err(e) if e.kind() == ErrorKind::NotFound => continue,
                    Err(_) => None,
                };
                any = true;
                let Some(names) = names else {
                    complete = false;
                    continue;
                };
                for name in names {
                    if cancel.is_cancelled() {
                        complete = false;
                        break 'prefixes;
                    }
                    match package(manager, &name, &dir.join(&name)) {
                        Some(package) => {
                            found.insert((manager, name), package);
                        }
                        // A folder without a version folder is a broken install.
                        None => complete = false,
                    }
                }
            }
        }
        let status = if !any {
            SourceStatus::Unavailable
        } else if complete {
            SourceStatus::Complete
        } else {
            SourceStatus::Partial
        };
        InventoryReport {
            source: source(),
            status,
            applications: Vec::new(),
            services: Vec::new(),
            packages: found.into_values().collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use lumen_domain::{PackageId, PackageRelationKind, RelationMethod};

    use super::*;

    fn write(path: &Path, text: &str) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, text)
    }

    #[test]
    fn formulae_and_casks_are_listed_apart_with_why_they_were_installed() -> std::io::Result<()> {
        let prefix = tempfile::tempdir()?;
        let p = prefix.path();
        write(
            &p.join("Cellar/act/0.2.79/INSTALL_RECEIPT.json"),
            r#"{"installed_on_request": true}"#,
        )?;
        write(
            &p.join("Cellar/openssl@3/3.4.0/INSTALL_RECEIPT.json"),
            r#"{"installed_on_request": false}"#,
        )?;
        write(
            &p.join("Cellar/openssl@3/3.5.0/INSTALL_RECEIPT.json"),
            r#"{"installed_on_request": false}"#,
        )?;
        write(
            &p.join("Cellar/docker/27.0/INSTALL_RECEIPT.json"),
            "not json",
        )?;
        write(&p.join("Caskroom/docker/4.30/Docker.app/x"), "")?;
        write(
            &p.join("Caskroom/docker/.metadata/INSTALL_RECEIPT.json"),
            r#"{"installed_on_request": true}"#,
        )?;
        let report =
            HomebrewInventory::in_prefixes(vec![p.to_path_buf()]).collect(&CancelToken::new());
        assert_eq!(report.status, SourceStatus::Complete);
        let found: Vec<_> = report
            .packages
            .iter()
            .map(|p| {
                (
                    p.manager,
                    p.id.raw().to_owned(),
                    p.version.as_ref().map(|v| v.raw().to_owned()),
                    p.explicit,
                )
            })
            .collect();
        assert_eq!(
            found,
            [
                (
                    PackageManager::Homebrew,
                    "act".into(),
                    Some("0.2.79".into()),
                    Some(true)
                ),
                (
                    PackageManager::Homebrew,
                    "docker".into(),
                    Some("27.0".into()),
                    None
                ),
                (
                    PackageManager::Homebrew,
                    "openssl@3".into(),
                    Some("3.5.0".into()),
                    Some(false)
                ),
                (
                    PackageManager::HomebrewCask,
                    "docker".into(),
                    Some("4.30".into()),
                    Some(true)
                ),
            ]
        );
        Ok(())
    }

    #[test]
    fn each_package_carries_what_its_records_say_it_needs_and_installed() -> std::io::Result<()> {
        let prefix = tempfile::tempdir()?;
        let p = prefix.path();
        write(
            &p.join("Cellar/act/0.2.79/INSTALL_RECEIPT.json"),
            r#"{"runtime_dependencies": [{"full_name": "openssl@3", "declared_directly": true}]}"#,
        )?;
        write(&p.join("Cellar/old/1.0/INSTALL_RECEIPT.json"), "{}")?;
        write(&p.join("Caskroom/docker/4.30/Docker.app/x"), "")?;
        write(
            &p.join("Caskroom/docker/.metadata/4.30/20250101000000.000/Casks/docker.json"),
            r#"{"artifacts": [{"app": ["Docker.app"]}]}"#,
        )?;
        write(
            &p.join("Caskroom/docker/.metadata/config.json"),
            r#"{"default": {"appdir": "/Applications"}}"#,
        )?;
        let report =
            HomebrewInventory::in_prefixes(vec![p.to_path_buf()]).collect(&CancelToken::new());
        let relations = |name: &str| {
            report
                .packages
                .iter()
                .find(|package| package.id.raw() == name)
                .and_then(|package| package.relations.clone())
        };
        assert_eq!(
            relations("act").map(|r| r.into_iter().map(|r| r.kind).collect::<Vec<_>>()),
            Some(vec![PackageRelationKind::DependsOn {
                package: PackageId {
                    manager: PackageManager::Homebrew,
                    id: UntrustedText::new("openssl@3"),
                },
                direct: Some(true),
            }])
        );
        // A receipt that does not list dependencies leaves them unknown.
        assert_eq!(relations("old"), None);
        assert_eq!(
            relations("docker").map(|r| r.into_iter().map(|r| r.method).collect::<Vec<_>>()),
            Some(vec![RelationMethod::CaskDefinition])
        );
        Ok(())
    }

    #[test]
    fn a_folder_without_versions_makes_the_list_partial() -> std::io::Result<()> {
        let prefix = tempfile::tempdir()?;
        std::fs::create_dir_all(prefix.path().join("Cellar/broken"))?;
        let report = HomebrewInventory::in_prefixes(vec![prefix.path().to_path_buf()])
            .collect(&CancelToken::new());
        assert_eq!(report.status, SourceStatus::Partial);
        assert_eq!(report.packages.len(), 0);
        Ok(())
    }

    #[test]
    fn no_homebrew_is_unavailable_not_empty() -> std::io::Result<()> {
        let prefix = tempfile::tempdir()?;
        let report = HomebrewInventory::in_prefixes(vec![prefix.path().to_path_buf()])
            .collect(&CancelToken::new());
        assert_eq!(report.status, SourceStatus::Unavailable);
        Ok(())
    }
}
