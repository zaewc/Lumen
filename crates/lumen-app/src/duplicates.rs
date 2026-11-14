//! Exact duplicates (ADR-0019): in a completed scan, from its recorded sizes
//! ([`find_in_scan`]), or in roots the user names, from a fresh read-only
//! listing ([`find`]); then the staged comparison in
//! `lumen_application::duplicates`. A scan is compared one size at a time, so
//! there is no limit on how many files it holds.
//!
//! This is the only place file contents are read, and only for regular files
//! outside protected locations and quarantine stores. The result is review
//! evidence; it never selects a copy to keep or changes anything.

use std::ops::ControlFlow;
use std::path::{Path, PathBuf};

use lumen_application::CancelToken;
use lumen_application::duplicates::{Detector, FingerprintReader};
use lumen_domain::{EntryKind, FilesystemEntry, Platform, ScanId};
use lumen_policy::{Context, protected_location};
use lumen_scan::{ScanItem, scan as run_scan};
use lumen_store_sqlite::{IndexStore, StoreError, open_index};

pub use lumen_application::duplicates::{DuplicateGroup, DuplicateReport, HashFailure, HashStage};

use crate::scan::{ScanError, absolute_roots, enumerator, excluding_stores};

/// Why no duplicate report could be made.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum DuplicatesError {
    /// A root is unusable or quarantine stores are unknown.
    #[error(transparent)]
    Scan(#[from] ScanError),
    /// The scan's records could not be read.
    #[error(transparent)]
    Store(Box<StoreError>),
    /// No content reader exists for this platform yet.
    #[error("duplicate analysis is not available on this platform yet")]
    Unsupported,
}

impl From<StoreError> for DuplicatesError {
    fn from(error: StoreError) -> Self {
        Self::Store(Box::new(error))
    }
}

/// A duplicate report and what the listing could not cover.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Duplicates {
    /// Verified groups, hard links and per-file skips.
    pub report: DuplicateReport,
    /// Files left unread because they are in a protected location.
    pub protected: usize,
    /// Some directories were unreadable, excluded or not listed completely;
    /// files there were not compared (ADR-0008: unknown, not absent).
    pub incomplete: bool,
}

/// Whether this platform can read contents to compare; elsewhere [`find`]
/// says [`DuplicatesError::Unsupported`].
#[must_use]
pub const fn available() -> bool {
    cfg!(any(target_os = "macos", windows, target_os = "linux"))
}

/// Find exact duplicates below `roots`, reporting each attempted fingerprint.
///
/// Roots are resolved to their real paths first: contents are opened with no
/// symlink in any component, so `/tmp/x` is read as `/private/tmp/x`.
///
/// # Errors
///
/// [`DuplicatesError`] if a root is unusable or the platform has no reader.
pub fn find(
    roots: &[PathBuf],
    cancel: &CancelToken,
    progress: &mut dyn FnMut(HashStage, usize),
) -> Result<Duplicates, DuplicatesError> {
    if !available() {
        return Err(DuplicatesError::Unsupported);
    }
    let roots = roots
        .iter()
        .map(|root| {
            std::fs::canonicalize(root)
                .map(plain)
                .map_err(|source| ScanError::Root {
                    root: root.clone(),
                    source,
                })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let stores = crate::store::scan_exclusion_paths().map_err(ScanError::from)?;
    let config = excluding_stores(&roots, &stores)?;
    let context = context();
    let mut entries: Vec<FilesystemEntry> = Vec::new();
    let mut protected = 0;
    let mut incomplete = false;
    let summary = run_scan(
        &enumerator(),
        &absolute_roots(&roots)?,
        &config,
        cancel,
        |item| {
            match item {
                ScanItem::Entry(entry) if entry.kind == EntryKind::File => {
                    if protected_location(&entry.path, &context).is_some() {
                        protected += 1;
                    } else {
                        entries.push(entry);
                    }
                }
                ScanItem::Listed { outcome, .. } if !outcome.complete => incomplete = true,
                ScanItem::Skipped { .. } => incomplete = true,
                _ => {}
            }
            ControlFlow::Continue(())
        },
    );
    if summary.stop.is_some() {
        return Ok(Duplicates {
            report: DuplicateReport {
                cancelled: true,
                ..DuplicateReport::default()
            },
            protected,
            incomplete: true,
        });
    }
    Ok(Duplicates {
        report: read(&entries, cancel, progress)?,
        protected,
        incomplete,
    })
}

fn read(
    entries: &[FilesystemEntry],
    cancel: &CancelToken,
    progress: &mut dyn FnMut(HashStage, usize),
) -> Result<DuplicateReport, DuplicatesError> {
    with_reader(|reader| lumen_application::duplicates::detect(entries, reader, cancel, progress))
}

/// Exact duplicates among the files of completed scan `scan` in `db`, compared
/// one size at a time from the sizes the scan recorded, so memory holds one
/// group of same-size files, however many files the scan has. Files in a
/// protected location are left unread; each read checks the file is still the
/// one the scan saw, and skips it otherwise.
///
/// # Errors
///
/// [`DuplicatesError`] if the scan cannot be read or the platform has no
/// reader.
pub fn find_in_scan(
    db: &Path,
    scan: ScanId,
    cancel: &CancelToken,
    progress: &mut dyn FnMut(HashStage, usize),
) -> Result<Duplicates, DuplicatesError> {
    if !available() {
        return Err(DuplicatesError::Unsupported);
    }
    let store = IndexStore::new(open_index(db)?);
    let incomplete = !store.unread_directories(scan)?.is_empty();
    let real = real_roots(&store.scan_roots(scan)?);
    let context = context();
    with_reader(|reader| -> Result<Duplicates, DuplicatesError> {
        let mut detector = Detector::new(reader, cancel);
        let mut protected = 0;
        store.same_size_files(scan, &mut |files| {
            let (readable, left): (Vec<_>, Vec<_>) = files
                .into_iter()
                .map(|file| at_real(file, &real))
                .partition(|file| protected_location(&file.path, &context).is_none());
            protected += left.len();
            detector.add(&readable, progress)
        })?;
        let report = detector.finish();
        Ok(Duplicates {
            incomplete: incomplete || report.cancelled,
            report,
            protected,
        })
    })?
}

/// Each scan root whose real path differs, with that real path. The scan
/// records paths as it was given them, and the reader opens no path through a
/// symlink (`/var` is one on macOS), so files are read at their real path, as
/// [`find`] reads them. Only Unix paths; a Windows root is kept as it is.
#[cfg(unix)]
fn real_roots(roots: &[lumen_domain::RawPath]) -> Vec<(Vec<u8>, Vec<u8>)> {
    use std::os::unix::ffi::{OsStrExt as _, OsStringExt as _};
    roots
        .iter()
        .filter(|root| root.flavor() == lumen_domain::PathFlavor::Unix)
        .filter_map(|root| {
            let real = std::fs::canonicalize(std::ffi::OsStr::from_bytes(root.as_bytes()))
                .ok()?
                .into_os_string()
                .into_vec();
            (real != root.as_bytes()).then(|| (root.as_bytes().to_vec(), real))
        })
        .collect()
}

#[cfg(not(unix))]
fn real_roots(_: &[lumen_domain::RawPath]) -> Vec<(Vec<u8>, Vec<u8>)> {
    Vec::new()
}

/// `entry` at its real path, when it is below one of the `real` roots.
fn at_real(mut entry: FilesystemEntry, real: &[(Vec<u8>, Vec<u8>)]) -> FilesystemEntry {
    for (root, target) in real {
        let Some(rest) = entry.path.as_bytes().strip_prefix(root.as_slice()) else {
            continue;
        };
        if !(rest.is_empty() || rest.starts_with(b"/") || root.ends_with(b"/")) {
            continue;
        }
        let mut moved = target.clone();
        moved.extend_from_slice(rest);
        if let Ok(path) = lumen_domain::RawPath::from_unix_bytes(moved) {
            entry.path = path;
        }
        break;
    }
    entry
}

/// Runs `compare` with this platform's content reader.
#[cfg(target_os = "macos")]
#[allow(
    clippy::unnecessary_wraps,
    reason = "other platforms have no reader and say so"
)]
fn with_reader<T>(compare: impl FnOnce(&dyn FingerprintReader) -> T) -> Result<T, DuplicatesError> {
    Ok(compare(&lumen_platform_macos::DuplicateReader))
}

/// Runs `compare` with this platform's content reader.
#[cfg(windows)]
#[allow(
    clippy::unnecessary_wraps,
    reason = "other platforms have no reader and say so"
)]
fn with_reader<T>(compare: impl FnOnce(&dyn FingerprintReader) -> T) -> Result<T, DuplicatesError> {
    Ok(compare(&lumen_platform_windows::DuplicateReader))
}

/// Runs `compare` with this platform's content reader.
#[cfg(target_os = "linux")]
#[allow(
    clippy::unnecessary_wraps,
    reason = "other platforms have no reader and say so"
)]
fn with_reader<T>(compare: impl FnOnce(&dyn FingerprintReader) -> T) -> Result<T, DuplicatesError> {
    Ok(compare(&lumen_platform_linux::DuplicateReader))
}

#[cfg(not(any(target_os = "macos", windows, target_os = "linux")))]
fn with_reader<T>(_: impl FnOnce(&dyn FingerprintReader) -> T) -> Result<T, DuplicatesError> {
    Err(DuplicatesError::Unsupported)
}

/// `canonicalize` returns `\\?\C:\…` on Windows; the scanner adds that prefix
/// itself, so a drive path is kept as people write it. Other paths, and every
/// path elsewhere, are unchanged.
#[cfg(windows)]
fn plain(path: PathBuf) -> PathBuf {
    use std::os::windows::ffi::{OsStrExt as _, OsStringExt as _};
    let wide: Vec<u16> = path.as_os_str().encode_wide().collect();
    let prefix: Vec<u16> = r"\\?\".encode_utf16().collect();
    let verbatim_drive = wide.len() >= 7
        && wide.starts_with(&prefix)
        && matches!(wide[4], 0x41..=0x5A | 0x61..=0x7A)
        && wide[5] == u16::from(b':')
        && wide[6] == u16::from(b'\\');
    if verbatim_drive {
        PathBuf::from(std::ffi::OsString::from_wide(&wide[4..]))
    } else {
        path
    }
}

#[cfg(not(windows))]
const fn plain(path: PathBuf) -> PathBuf {
    path
}

/// Only the roots protected locations are relative to; nothing here acts.
fn context() -> Context {
    let path = |path: Option<PathBuf>| path.and_then(|path| crate::raw_path(&path).ok());
    Context {
        platform: if cfg!(target_os = "macos") {
            Platform::Macos
        } else if cfg!(windows) {
            Platform::Windows
        } else {
            Platform::Linux
        },
        home: path(std::env::home_dir()),
        windows_dir: path(std::env::var_os("WINDIR").map(PathBuf::from)),
        local_app_data: path(std::env::var_os("LOCALAPPDATA").map(PathBuf::from)),
        mechanisms: std::collections::BTreeSet::new(),
        complete_sources: std::collections::BTreeSet::new(),
        single_user: false,
    }
}

#[cfg(test)]
mod tests {
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    use lumen_testkit::{Fixture, Node};

    use super::*;

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    fn file(path: &str, contents: &[u8]) -> Node {
        Node::File {
            path: path.into(),
            contents: contents.to_vec(),
        }
    }

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    #[test]
    fn copies_below_a_symlinked_root_are_grouped_for_review()
    -> Result<(), Box<dyn std::error::Error>> {
        let fx = Fixture::build(&[
            file("photos/a.jpg", &[9; 5000]),
            file("backup/a.jpg", &[9; 5000]),
            file("photos/b.jpg", &[8; 5000]),
            Node::HardLink {
                path: "photos/a-again.jpg".into(),
                existing: "photos/a.jpg".into(),
            },
        ])?;
        // On macOS the temporary directory is below `/var`, a symlink to
        // `/private/var`.
        let found = find(
            &[fx.root().to_path_buf()],
            &CancelToken::new(),
            &mut |_, _| {},
        )?;
        assert_eq!(found.report.groups.len(), 1);
        assert_eq!(found.report.groups[0].files.len(), 2);
        assert_eq!(found.report.hard_links.len(), 1);
        assert_eq!((found.protected, found.incomplete), (0, false));
        Ok(())
    }

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    #[test]
    fn a_scan_is_compared_from_its_recorded_sizes() -> Result<(), Box<dyn std::error::Error>> {
        let fx = Fixture::build(&[
            file("data/photos/a.jpg", &[9; 5000]),
            file("data/backup/a.jpg", &[9; 5000]),
            file("data/photos/b.jpg", &[8; 5000]),
            file("data/alone.txt", &[7; 12]),
        ])?;
        let db = fx.path("index.db")?;
        // On macOS the temporary directory is below `/var`, a symlink: the
        // comparison reads below the root's real path.
        let scanned = crate::scan(
            &[fx.path("data")?],
            Some(&db),
            &crate::SystemClock,
            &crate::UuidV7Ids,
            &CancelToken::new(),
            &mut |_| {},
        )?;
        let mut read = 0;
        let found = find_in_scan(&db, scanned.scan, &CancelToken::new(), &mut |_, done| {
            read = done;
        })?;
        assert_eq!(found.report.groups.len(), 1);
        let names: Vec<_> = found.report.groups[0]
            .files
            .iter()
            .map(|f| String::from_utf8_lossy(f.path.as_bytes()).into_owned())
            .collect();
        assert!(names.iter().all(|n| n.ends_with("/a.jpg")), "{names:?}");
        assert_eq!(
            read,
            3 + 2 + 2,
            "heads of three, then the two that match; never alone.txt"
        );
        assert_eq!((found.protected, found.incomplete), (0, false));
        let cancel = CancelToken::new();
        cancel.cancel();
        let stopped = find_in_scan(&db, scanned.scan, &cancel, &mut |_, _| {})?;
        assert!(stopped.report.cancelled && stopped.report.groups.is_empty());
        Ok(())
    }

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    #[test]
    fn cancellation_and_missing_roots_report_nothing() -> Result<(), Box<dyn std::error::Error>> {
        let fx = Fixture::build(&[file("a", &[1; 10]), file("b", &[1; 10])])?;
        let cancel = CancelToken::new();
        cancel.cancel();
        let found = find(&[fx.root().to_path_buf()], &cancel, &mut |_, _| {})?;
        assert!(found.report.cancelled && found.report.groups.is_empty());
        assert!(matches!(
            find(
                &[fx.root().join("missing")],
                &CancelToken::new(),
                &mut |_, _| {}
            ),
            Err(DuplicatesError::Scan(_))
        ));
        Ok(())
    }

    #[cfg(windows)]
    #[test]
    fn drive_paths_lose_only_the_verbatim_prefix() {
        assert_eq!(
            plain(PathBuf::from(r"\\?\C:\Users\ana")),
            PathBuf::from(r"C:\Users\ana")
        );
        for kept in [r"\\?\UNC\srv\share", r"C:\Users\ana", r"\\?\Volume{x}\"] {
            assert_eq!(plain(PathBuf::from(kept)), PathBuf::from(kept));
        }
        assert!(matches!(
            find(
                &[PathBuf::from(r"C:\no\such\folder")],
                &CancelToken::new(),
                &mut |_, _| {}
            ),
            Err(DuplicatesError::Scan(_))
        ));
    }

    #[cfg(not(any(target_os = "macos", windows, target_os = "linux")))]
    #[test]
    fn other_platforms_say_unsupported_instead_of_finding_nothing() {
        assert!(matches!(
            find(&[], &CancelToken::new(), &mut |_, _| {}),
            Err(DuplicatesError::Unsupported)
        ));
    }
}
