//! Installed packages from the package managers' own databases (#806,
//! ADR-0060): dpkg's status file and pacman's local database, read as files.
//! Nothing is run and nothing is written.
//!
//! The parsers are plain functions, tested on every platform; on Linux,
//! [`DpkgInventory`] and [`PacmanInventory`] read the default locations. A
//! manager whose database is missing reports
//! [`SourceStatus::Unavailable`](lumen_domain::SourceStatus): it is not
//! installed, or keeps its database elsewhere. A record that cannot be parsed
//! makes the report partial rather than being guessed at.

use lumen_domain::{Package, PackageManager, UntrustedText};

/// The `name: value` fields of one control paragraph. Continuation lines
/// (starting with a space or tab) belong to the previous field and are not
/// needed here, so they are skipped.
fn fields(paragraph: &str) -> impl Iterator<Item = (&str, &str)> {
    paragraph
        .lines()
        .filter(|line| !line.starts_with([' ', '\t']))
        .filter_map(|line| line.split_once(':'))
        .map(|(name, value)| (name.trim(), value.trim()))
}

/// dpkg states in which a package's files are on disk. `not-installed` and
/// `config-files` (removed, configuration kept) are not installed software.
const DPKG_PRESENT: [&str; 6] = [
    "installed",
    "unpacked",
    "half-configured",
    "half-installed",
    "triggers-awaited",
    "triggers-pending",
];

/// The installed packages in dpkg's status file (`/var/lib/dpkg/status`), and
/// how many paragraphs could not be read as a package.
///
/// A package is identified as dpkg does: by name, with its architecture unless
/// it is `all`, so `libc6:amd64` and `libc6:i386` stay apart.
#[must_use]
pub fn parse_dpkg_status(status: &str) -> (Vec<Package>, usize) {
    let mut packages = Vec::new();
    let mut malformed = 0;
    for paragraph in status.split("\n\n").filter(|p| !p.trim().is_empty()) {
        let mut name = None;
        let mut state = None;
        let mut version = None;
        let mut arch = None;
        let mut required = false;
        for (field, value) in fields(paragraph) {
            match field {
                "Package" => name = Some(value),
                // `want flag state`, e.g. `install ok installed`.
                "Status" => state = value.split_whitespace().nth(2),
                "Version" => version = Some(value),
                "Architecture" => arch = Some(value),
                "Essential" => required |= value == "yes",
                "Priority" => required |= value == "required",
                _ => {}
            }
        }
        let (Some(name), Some(state)) = (name.filter(|n| !n.is_empty()), state) else {
            malformed += 1;
            continue;
        };
        if !DPKG_PRESENT.contains(&state) {
            continue;
        }
        let id = match arch.filter(|a| !a.is_empty() && *a != "all") {
            Some(arch) => format!("{name}:{arch}"),
            None => name.to_owned(),
        };
        packages.push(Package {
            manager: PackageManager::Dpkg,
            id: UntrustedText::new(id),
            version: version.filter(|v| !v.is_empty()).map(UntrustedText::new),
            install_root: None,
            required,
            // dpkg does not record why a package was installed; apt keeps
            // that in another file.
            explicit: None,
        });
    }
    (packages, malformed)
}

/// The package one pacman `desc` file (`/var/lib/pacman/local/<pkg>/desc`)
/// describes, or `None` if it names none.
#[must_use]
pub fn parse_pacman_desc(desc: &str) -> Option<Package> {
    let mut name = None;
    let mut version = None;
    let mut reason = None;
    let mut lines = desc.lines().map(str::trim);
    while let Some(line) = lines.next() {
        let value = || lines.clone().next().filter(|v| !v.is_empty());
        match line {
            "%NAME%" => name = value(),
            "%VERSION%" => version = value(),
            "%REASON%" => reason = value(),
            _ => {}
        }
    }
    Some(Package {
        manager: PackageManager::Pacman,
        id: UntrustedText::new(name?),
        version: version.map(UntrustedText::new),
        install_root: None,
        // pacman has no "part of the base system" mark.
        required: false,
        // `%REASON%` 1 means installed as a dependency; absent or 0 means
        // installed explicitly.
        explicit: Some(reason != Some("1")),
    })
}

#[cfg(target_os = "linux")]
pub use inventories::{DpkgInventory, PacmanInventory};

#[cfg(target_os = "linux")]
mod inventories {
    use std::io::{ErrorKind, Read as _};
    use std::path::Path;

    use lumen_application::CancelToken;
    use lumen_application::ports::{InventoryReport, InventorySource};
    use lumen_domain::{Package, SourceName, SourceStatus};

    use super::{parse_dpkg_status, parse_pacman_desc};

    /// The largest database file read; a larger one is not read as a whole and
    /// the report is partial.
    const MAX_FILE: u64 = 64 * 1024 * 1024;

    fn source(name: &str) -> SourceName {
        SourceName::new(name).unwrap_or_else(|_| unreachable!("a valid literal"))
    }

    fn report(name: &str, status: SourceStatus, packages: Vec<Package>) -> InventoryReport {
        InventoryReport {
            source: source(name),
            status,
            applications: Vec::new(),
            services: Vec::new(),
            packages,
        }
    }

    /// A file's text, `Ok(None)` if it does not exist, `Err` if it cannot be
    /// read whole.
    fn read(path: &Path) -> Result<Option<String>, ()> {
        let file = match std::fs::File::open(path) {
            Ok(file) => file,
            Err(e) if e.kind() == ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err(()),
        };
        let mut text = String::new();
        let read = file
            .take(MAX_FILE + 1)
            .read_to_string(&mut text)
            .map_err(|_| ())?;
        if u64::try_from(read).unwrap_or(u64::MAX) > MAX_FILE {
            return Err(());
        }
        Ok(Some(text))
    }

    /// Packages from dpkg's status file, source `linux.dpkg`.
    #[derive(Debug, Clone, Copy, Default)]
    pub struct DpkgInventory;

    const DPKG: &str = "linux.dpkg";
    const DPKG_STATUS: &str = "/var/lib/dpkg/status";

    impl InventorySource for DpkgInventory {
        fn name(&self) -> SourceName {
            source(DPKG)
        }

        fn collect(&self, _cancel: &CancelToken) -> InventoryReport {
            match read(Path::new(DPKG_STATUS)) {
                Ok(None) => report(DPKG, SourceStatus::Unavailable, Vec::new()),
                Err(()) => report(DPKG, SourceStatus::Partial, Vec::new()),
                Ok(Some(text)) => {
                    let (packages, malformed) = parse_dpkg_status(&text);
                    let status = if malformed == 0 {
                        SourceStatus::Complete
                    } else {
                        SourceStatus::Partial
                    };
                    report(DPKG, status, packages)
                }
            }
        }
    }

    /// Packages from pacman's local database, source `linux.pacman`.
    #[derive(Debug, Clone, Copy, Default)]
    pub struct PacmanInventory;

    const PACMAN: &str = "linux.pacman";
    const PACMAN_LOCAL: &str = "/var/lib/pacman/local";

    impl InventorySource for PacmanInventory {
        fn name(&self) -> SourceName {
            source(PACMAN)
        }

        fn collect(&self, cancel: &CancelToken) -> InventoryReport {
            let entries = match std::fs::read_dir(PACMAN_LOCAL) {
                Ok(entries) => entries,
                Err(e) if e.kind() == ErrorKind::NotFound => {
                    return report(PACMAN, SourceStatus::Unavailable, Vec::new());
                }
                Err(_) => return report(PACMAN, SourceStatus::Partial, Vec::new()),
            };
            let mut packages = Vec::new();
            let mut complete = true;
            for entry in entries {
                if cancel.is_cancelled() {
                    complete = false;
                    break;
                }
                let Ok(entry) = entry else {
                    complete = false;
                    continue;
                };
                // `ALPM_DB_VERSION` is a file beside the package directories.
                if !entry.file_type().is_ok_and(|t| t.is_dir()) {
                    continue;
                }
                match read(&entry.path().join("desc")).map(|t| t.as_deref().map(parse_pacman_desc))
                {
                    Ok(Some(Some(package))) => packages.push(package),
                    _ => complete = false,
                }
            }
            let status = if complete {
                SourceStatus::Complete
            } else {
                SourceStatus::Partial
            };
            report(PACMAN, status, packages)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const STATUS: &str = "\
Package: libc6
Status: install ok installed
Priority: required
Architecture: amd64
Version: 2.39-0ubuntu8
Description: GNU C Library
 continuation: not a field

Package: base-files
Essential: yes
Status: install ok installed
Architecture: amd64
Version: 13ubuntu10

Package: libc6
Status: install ok installed
Architecture: i386
Version: 2.39-0ubuntu8

Package: tzdata
Status: install ok installed
Architecture: all
Version: 2024a-1

Package: old-tool
Status: deinstall ok config-files
Architecture: amd64
Version: 1.0

Package: half
Status: install reinstreq half-installed
Architecture: amd64

Status: install ok installed
Version: 9

Package: nostatus
Version: 1
";

    fn summary(packages: &[Package]) -> Vec<(String, Option<String>, bool)> {
        packages
            .iter()
            .map(|p| {
                (
                    p.id.raw().to_owned(),
                    p.version.as_ref().map(|v| v.raw().to_owned()),
                    p.required,
                )
            })
            .collect()
    }

    #[test]
    fn dpkg_lists_present_packages_by_name_and_architecture() {
        let (packages, malformed) = parse_dpkg_status(STATUS);
        assert_eq!(
            summary(&packages),
            [
                ("libc6:amd64".into(), Some("2.39-0ubuntu8".into()), true),
                ("base-files:amd64".into(), Some("13ubuntu10".into()), true),
                ("libc6:i386".into(), Some("2.39-0ubuntu8".into()), false),
                ("tzdata".into(), Some("2024a-1".into()), false),
                ("half:amd64".into(), None, false),
            ]
        );
        assert!(packages.iter().all(|p| p.manager == PackageManager::Dpkg));
        assert!(packages.iter().all(|p| p.explicit.is_none()));
        // One paragraph without a name, one without a status.
        assert_eq!(malformed, 2);
    }

    #[test]
    fn an_empty_status_file_lists_nothing() {
        assert_eq!(parse_dpkg_status(""), (Vec::new(), 0));
        assert_eq!(parse_dpkg_status("\n\n\n").1, 0);
    }

    #[test]
    fn pacman_records_name_version_and_why_it_was_installed() {
        let explicit = "%NAME%\nfirefox\n\n%VERSION%\n132.0-1\n\n%BASE%\nfirefox\n";
        let dependency = "%NAME%\nnss\n\n%VERSION%\n3.105-1\n\n%REASON%\n1\n";
        let found = [explicit, dependency].map(parse_pacman_desc);
        assert_eq!(
            found
                .iter()
                .flatten()
                .map(|p| (
                    p.id.raw().to_owned(),
                    p.version.as_ref().map(|v| v.raw().to_owned()),
                    p.explicit
                ))
                .collect::<Vec<_>>(),
            [
                ("firefox".into(), Some("132.0-1".into()), Some(true)),
                ("nss".into(), Some("3.105-1".into()), Some(false)),
            ]
        );
        assert!(
            found
                .iter()
                .flatten()
                .all(|p| p.manager == PackageManager::Pacman && !p.required)
        );
    }

    #[test]
    fn a_pacman_record_without_a_name_is_none() {
        assert_eq!(parse_pacman_desc("%VERSION%\n1.0-1\n"), None);
        assert_eq!(parse_pacman_desc("%NAME%\n\n%VERSION%\n1\n"), None);
        assert_eq!(parse_pacman_desc(""), None);
    }
}

#[cfg(all(test, target_os = "linux"))]
mod linux_tests {
    use lumen_application::CancelToken;
    use lumen_application::ports::InventorySource as _;
    use lumen_domain::SourceStatus;

    use super::{DpkgInventory, PacmanInventory};

    #[test]
    fn the_runners_package_database_is_read() {
        // CI's Linux runner is Ubuntu: dpkg is there, pacman is not.
        let dpkg = DpkgInventory.collect(&CancelToken::new());
        assert_eq!(dpkg.source.as_str(), "linux.dpkg");
        assert_eq!(dpkg.status, SourceStatus::Complete);
        assert!(
            dpkg.packages
                .iter()
                .any(|p| p.id.raw().starts_with("base-files") && p.required),
            "base-files is Essential"
        );
        assert!(
            dpkg.packages
                .iter()
                .any(|p| p.id.raw().starts_with("libc6:"))
        );
        let pacman = PacmanInventory.collect(&CancelToken::new());
        assert_eq!(pacman.source.as_str(), "linux.pacman");
        if std::path::Path::new("/var/lib/pacman/local").exists() {
            assert_ne!(pacman.status, SourceStatus::Unavailable);
        } else {
            assert_eq!(pacman.status, SourceStatus::Unavailable);
        }
    }
}
