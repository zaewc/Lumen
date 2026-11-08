//! Flatpak applications and snaps (#806, ADR-0060), read from the folders
//! each keeps its installations in. Nothing is run and nothing is written.
//!
//! - Flatpak: `app/<id>/current/active/metadata` under the system
//!   installation (`/var/lib/flatpak`) and the user's
//!   (`~/.local/share/flatpak`). The same application can be in both; each is
//!   its own package, told apart by its install root. Runtimes are shared
//!   components, not applications, and are not listed here.
//! - Snap: `<name>/current/meta/snap.yaml` under `/snap` (Ubuntu) or
//!   `/var/lib/snapd/snap`. Base, OS, kernel, gadget and `snapd` snaps run
//!   the system: they are `required`.
//!
//! A store whose folder is missing is not installed; one whose entries cannot
//! be read makes the report partial.

use std::collections::BTreeMap;
use std::io::{ErrorKind, Read as _};
use std::os::unix::ffi::OsStrExt as _;
use std::path::{Path, PathBuf};

use lumen_application::CancelToken;
use lumen_application::ports::{InventoryReport, InventorySource};
use lumen_domain::{Package, PackageManager, RawPath, SourceName, SourceStatus, UntrustedText};

/// The largest metadata file read.
const MAX_METADATA: u64 = 256 * 1024;

fn read_small(path: &Path) -> Option<String> {
    let mut text = String::new();
    std::fs::File::open(path)
        .ok()?
        .take(MAX_METADATA)
        .read_to_string(&mut text)
        .ok()?;
    Some(text)
}

fn raw(path: &Path) -> Option<RawPath> {
    RawPath::from_unix_bytes(path.as_os_str().as_bytes().to_vec()).ok()
}

/// The names of the subdirectories of `dir`, sorted; `Ok(None)` if `dir` does
/// not exist, `Err` if it cannot be listed.
fn subdirectories(dir: &Path) -> Result<Option<Vec<String>>, ()> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(()),
    };
    let mut names = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|_| ())?;
        if entry
            .file_type()
            .is_ok_and(|t| t.is_dir() || t.is_symlink())
            && let Ok(name) = entry.file_name().into_string()
        {
            names.push(name);
        }
    }
    names.sort();
    Ok(Some(names))
}

fn report(source: &str, any: bool, complete: bool, packages: Vec<Package>) -> InventoryReport {
    InventoryReport {
        source: SourceName::new(source).unwrap_or_else(|_| unreachable!("a valid literal")),
        status: if !any {
            SourceStatus::Unavailable
        } else if complete {
            SourceStatus::Complete
        } else {
            SourceStatus::Partial
        },
        applications: Vec::new(),
        services: Vec::new(),
        packages,
    }
}

/// The application ID a Flatpak `metadata` file names in its `[Application]`
/// group; `None` if it names none.
#[must_use]
pub fn flatpak_app_id(metadata: &str) -> Option<&str> {
    let mut in_application = false;
    for line in metadata.lines().map(str::trim) {
        if line.starts_with('[') {
            in_application = line == "[Application]";
        } else if in_application
            && let Some(("name", value)) = line.split_once('=').map(|(k, v)| (k.trim(), v.trim()))
        {
            return Some(value).filter(|v| !v.is_empty());
        }
    }
    None
}

/// Flatpak applications in the system and the user's installation, source
/// `linux.flatpak`.
#[derive(Debug, Clone)]
pub struct FlatpakInventory {
    installations: Vec<PathBuf>,
}

impl FlatpakInventory {
    /// The system installation and the current user's.
    #[must_use]
    pub fn new() -> Self {
        let mut installations = vec![PathBuf::from("/var/lib/flatpak")];
        if let Some(home) = std::env::var_os("HOME").filter(|h| !h.is_empty()) {
            installations.push(PathBuf::from(home).join(".local/share/flatpak"));
        }
        Self::in_installations(installations)
    }

    /// Other installations, for tests.
    #[must_use]
    pub const fn in_installations(installations: Vec<PathBuf>) -> Self {
        Self { installations }
    }
}

impl Default for FlatpakInventory {
    fn default() -> Self {
        Self::new()
    }
}

impl InventorySource for FlatpakInventory {
    fn name(&self) -> SourceName {
        SourceName::new("linux.flatpak").unwrap_or_else(|_| unreachable!("a valid literal"))
    }

    fn collect(&self, cancel: &CancelToken) -> InventoryReport {
        let mut packages = Vec::new();
        let (mut any, mut complete) = (false, true);
        'installations: for installation in &self.installations {
            let apps = installation.join("app");
            let names = match subdirectories(&apps) {
                Ok(None) => continue,
                Ok(Some(names)) => names,
                Err(()) => {
                    any = true;
                    complete = false;
                    continue;
                }
            };
            any = true;
            for name in names {
                if cancel.is_cancelled() {
                    complete = false;
                    break 'installations;
                }
                let dir = apps.join(&name);
                let metadata = read_small(&dir.join("current/active/metadata"));
                // The folder is named after the application; its metadata
                // must agree, or the entry is not trusted.
                match metadata.as_deref().and_then(flatpak_app_id) {
                    Some(id) if id == name => packages.push(Package {
                        manager: PackageManager::Flatpak,
                        id: UntrustedText::new(id),
                        version: None,
                        install_root: raw(&dir),
                        required: false,
                        // Applications are installed on request; runtimes
                        // come with them and are not listed.
                        explicit: Some(true),
                    }),
                    _ => complete = false,
                }
            }
        }
        report("linux.flatpak", any, complete, packages)
    }
}

/// A snap's `name`, `version` and `type` from its `meta/snap.yaml` (top-level
/// `key: value` lines only); `None` without a name.
#[must_use]
pub fn snap_meta(yaml: &str) -> Option<(&str, Option<&str>, Option<&str>)> {
    let mut name = None;
    let mut version = None;
    let mut kind = None;
    for line in yaml.lines().filter(|l| !l.starts_with([' ', '\t', '#'])) {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let value = value.trim().trim_matches(['\'', '"']);
        let value = Some(value).filter(|v| !v.is_empty());
        match key.trim() {
            "name" => name = value,
            "version" => version = value,
            "type" => kind = value,
            _ => {}
        }
    }
    Some((name?, version, kind))
}

/// Snap types that run the system rather than an application.
const SYSTEM_SNAPS: [&str; 5] = ["base", "os", "kernel", "gadget", "snapd"];

/// Installed snaps, source `linux.snap`.
#[derive(Debug, Clone)]
pub struct SnapInventory {
    roots: Vec<PathBuf>,
}

impl SnapInventory {
    /// `/snap` and `/var/lib/snapd/snap`.
    #[must_use]
    pub fn new() -> Self {
        Self::in_roots(vec![
            PathBuf::from("/snap"),
            PathBuf::from("/var/lib/snapd/snap"),
        ])
    }

    /// Other mount roots, for tests.
    #[must_use]
    pub const fn in_roots(roots: Vec<PathBuf>) -> Self {
        Self { roots }
    }
}

impl Default for SnapInventory {
    fn default() -> Self {
        Self::new()
    }
}

impl InventorySource for SnapInventory {
    fn name(&self) -> SourceName {
        SourceName::new("linux.snap").unwrap_or_else(|_| unreachable!("a valid literal"))
    }

    fn collect(&self, cancel: &CancelToken) -> InventoryReport {
        // The same snaps can be visible under both roots (a symlink).
        let mut found = BTreeMap::new();
        let (mut any, mut complete) = (false, true);
        'roots: for root in &self.roots {
            let names = match subdirectories(root) {
                Ok(None) => continue,
                Ok(Some(names)) => names,
                Err(()) => {
                    any = true;
                    complete = false;
                    continue;
                }
            };
            any = true;
            // `bin` holds the snaps' command links, not a snap.
            for name in names.into_iter().filter(|n| n != "bin") {
                if cancel.is_cancelled() {
                    complete = false;
                    break 'roots;
                }
                let dir = root.join(&name);
                let yaml = read_small(&dir.join("current/meta/snap.yaml"));
                match yaml.as_deref().and_then(snap_meta) {
                    Some((id, version, kind)) if id == name => {
                        found.entry(name.clone()).or_insert_with(|| Package {
                            manager: PackageManager::Snap,
                            id: UntrustedText::new(id),
                            version: version.map(UntrustedText::new),
                            install_root: raw(&dir),
                            required: kind.is_some_and(|k| SYSTEM_SNAPS.contains(&k)),
                            // snapd records this in its state, readable by
                            // root only.
                            explicit: None,
                        });
                    }
                    _ => complete = false,
                }
            }
        }
        report("linux.snap", any, complete, found.into_values().collect())
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
    fn flatpak_metadata_names_its_application() {
        let metadata = "[Application]\nname=org.example.App\nruntime=org.freedesktop.Platform/x86_64/23.08\n\n[Context]\nname=other\n";
        assert_eq!(flatpak_app_id(metadata), Some("org.example.App"));
        assert_eq!(
            flatpak_app_id("[Runtime]\nname=org.freedesktop.Platform\n"),
            None
        );
        assert_eq!(flatpak_app_id("[Application]\nname=\n"), None);
    }

    #[test]
    fn flatpak_lists_each_installations_applications() -> std::io::Result<()> {
        let system = tempfile::tempdir()?;
        let user = tempfile::tempdir()?;
        let app = "org.example.App";
        for root in [system.path(), user.path()] {
            write(
                &root.join("app").join(app).join("current/active/metadata"),
                &format!("[Application]\nname={app}\n"),
            )?;
        }
        let inventory = FlatpakInventory::in_installations(vec![
            system.path().to_path_buf(),
            user.path().to_path_buf(),
        ]);
        let report = inventory.collect(&CancelToken::new());
        assert_eq!(report.status, SourceStatus::Complete);
        assert_eq!(report.packages.len(), 2, "one per installation");
        assert!(
            report
                .packages
                .iter()
                .all(|p| p.id.raw() == app && p.explicit == Some(true))
        );
        assert_ne!(
            report.packages[0].install_root,
            report.packages[1].install_root
        );

        // A folder whose metadata names another application is not trusted.
        write(
            &user
                .path()
                .join("app/org.example.Other/current/active/metadata"),
            "[Application]\nname=org.example.App\n",
        )?;
        assert_eq!(
            inventory.collect(&CancelToken::new()).status,
            SourceStatus::Partial
        );
        Ok(())
    }

    #[test]
    fn snap_meta_reads_top_level_keys() {
        let yaml = "name: core22\nversion: '20241001'\ntype: base\napps:\n  name: nested\n";
        assert_eq!(
            snap_meta(yaml),
            Some(("core22", Some("20241001"), Some("base")))
        );
        assert_eq!(snap_meta("version: 1\n"), None);
    }

    #[test]
    fn snaps_are_listed_once_and_system_snaps_are_required() -> std::io::Result<()> {
        let root = tempfile::tempdir()?;
        let r = root.path();
        write(
            &r.join("firefox/current/meta/snap.yaml"),
            "name: firefox\nversion: 132.0\n",
        )?;
        write(
            &r.join("core22/current/meta/snap.yaml"),
            "name: core22\nversion: 2024\ntype: base\n",
        )?;
        std::fs::create_dir_all(r.join("bin"))?;
        let report = SnapInventory::in_roots(vec![r.to_path_buf(), r.to_path_buf()])
            .collect(&CancelToken::new());
        assert_eq!(report.status, SourceStatus::Complete);
        let found: Vec<_> = report
            .packages
            .iter()
            .map(|p| (p.id.raw().to_owned(), p.required))
            .collect();
        assert_eq!(found, [("core22".into(), true), ("firefox".into(), false)]);
        Ok(())
    }

    #[test]
    fn missing_stores_are_unavailable_not_empty() {
        let none = PathBuf::from("/nonexistent/lumen-store");
        let flatpak = FlatpakInventory::in_installations(vec![none.clone()]);
        assert_eq!(
            flatpak.collect(&CancelToken::new()).status,
            SourceStatus::Unavailable
        );
        let snap = SnapInventory::in_roots(vec![none]);
        assert_eq!(
            snap.collect(&CancelToken::new()).status,
            SourceStatus::Unavailable
        );
    }
}
