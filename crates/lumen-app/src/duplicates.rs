//! Exact duplicates in roots the user chose (ADR-0019): a fresh read-only
//! listing, then the staged comparison in `lumen_application::duplicates`.
//!
//! This is the only place file contents are read, and only for regular files
//! outside protected locations and quarantine stores. The result is review
//! evidence; it never selects a copy to keep or changes anything.

use std::ops::ControlFlow;
use std::path::PathBuf;

use lumen_application::CancelToken;
use lumen_domain::{EntryKind, FilesystemEntry, Platform};
use lumen_policy::{Context, protected_location};
use lumen_scan::{ScanItem, scan as run_scan};

pub use lumen_application::duplicates::{
    DuplicateGroup, DuplicateReport, HashFailure, HashStage, MAX_FILES, TooManyFiles,
};

use crate::scan::{ScanError, absolute_roots, enumerator, excluding_stores};

/// Why no duplicate report could be made.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum DuplicatesError {
    /// A root is unusable or quarantine stores are unknown.
    #[error(transparent)]
    Scan(#[from] ScanError),
    /// The chosen roots hold more files than the analysis is bounded to.
    #[error(transparent)]
    TooMany(#[from] TooManyFiles),
    /// No content reader exists for this platform yet.
    #[error("duplicate analysis is not available on this platform yet")]
    Unsupported,
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
/// [`DuplicatesError`] if a root is unusable, the scope has more than
/// [`MAX_FILES`] files, or the platform has no reader.
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
    let mut too_many = false;
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
                    } else if entries.len() == MAX_FILES {
                        too_many = true;
                        return ControlFlow::Break(());
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
    if too_many {
        return Err(TooManyFiles.into());
    }
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

#[cfg(target_os = "macos")]
fn read(
    entries: &[FilesystemEntry],
    cancel: &CancelToken,
    progress: &mut dyn FnMut(HashStage, usize),
) -> Result<DuplicateReport, DuplicatesError> {
    let reader = lumen_platform_macos::DuplicateReader;
    Ok(lumen_application::duplicates::detect(
        entries, &reader, cancel, progress,
    )?)
}

#[cfg(windows)]
fn read(
    entries: &[FilesystemEntry],
    cancel: &CancelToken,
    progress: &mut dyn FnMut(HashStage, usize),
) -> Result<DuplicateReport, DuplicatesError> {
    let reader = lumen_platform_windows::DuplicateReader;
    Ok(lumen_application::duplicates::detect(
        entries, &reader, cancel, progress,
    )?)
}

#[cfg(target_os = "linux")]
fn read(
    entries: &[FilesystemEntry],
    cancel: &CancelToken,
    progress: &mut dyn FnMut(HashStage, usize),
) -> Result<DuplicateReport, DuplicatesError> {
    let reader = lumen_platform_linux::DuplicateReader;
    Ok(lumen_application::duplicates::detect(
        entries, &reader, cancel, progress,
    )?)
}

#[cfg(not(any(target_os = "macos", windows, target_os = "linux")))]
fn read(
    _: &[FilesystemEntry],
    _: &CancelToken,
    _: &mut dyn FnMut(HashStage, usize),
) -> Result<DuplicateReport, DuplicatesError> {
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
