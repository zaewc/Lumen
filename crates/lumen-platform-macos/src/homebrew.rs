//! Homebrew formulae and casks (#806, ADR-0060), read from Homebrew's own
//! folders: `Cellar/<formula>/<version>` and `Caskroom/<cask>/<version>`,
//! with each install receipt's `installed_on_request`. Nothing is run, so the
//! list does not depend on `brew` being on a `PATH`.
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

/// `installed_on_request` from an install receipt; `None` if the receipt is
/// missing, unreadable, too large, or does not say.
fn on_request(receipt: &Path) -> Option<bool> {
    let mut text = String::new();
    std::fs::File::open(receipt)
        .ok()?
        .take(MAX_RECEIPT)
        .read_to_string(&mut text)
        .ok()?;
    serde_json::from_str::<serde_json::Value>(&text)
        .ok()?
        .get("installed_on_request")?
        .as_bool()
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
/// installed version; the newest receipt says whether it was asked for.
fn package(manager: PackageManager, name: &str, dir: &Path) -> Option<Package> {
    let versions = subdirectories(dir)?;
    // Several versions of a formula can be installed side by side; the last
    // in name order is shown, and any receipt saying "on request" counts.
    let version = versions.last()?;
    let explicit = match manager {
        PackageManager::HomebrewCask => on_request(&dir.join(".metadata/INSTALL_RECEIPT.json")),
        _ => versions
            .iter()
            .filter_map(|v| on_request(&dir.join(v).join("INSTALL_RECEIPT.json")))
            .reduce(|a, b| a || b),
    };
    Some(Package {
        manager,
        id: UntrustedText::new(name),
        version: Some(UntrustedText::new(version.clone())),
        install_root: raw(dir),
        // Homebrew has no "part of the base system" mark.
        required: false,
        explicit,
        relations: Vec::new(),
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
