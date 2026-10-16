//! Read-only scans of chosen roots (roadmap 5.x, 11.2): the platform's best
//! enumerator, coverage and size aggregation, and, with a database, the
//! recorded scan and a snapshot of each root.

use std::ops::ControlFlow;
use std::path::{Path, PathBuf};

use lumen_application::CancelToken;
use lumen_application::ports::{Clock, DirEnumerator, IdGenerator};
use lumen_domain::{ByteCount, PathError, RawPath, RootCoverage, ScanId, ScanState};
use lumen_scan::{
    CoverageBuilder, ScanConfig, ScanItem, SizeAggregator, StopReason, scan as run_scan,
};
use lumen_store_sqlite::{
    IndexStore, IndexWriter, RootSnapshot, StoreError, WriterConfig, open_index,
};

/// What a scan found below one root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RootSummary {
    /// The root, as scanned.
    pub root: RawPath,
    /// Files below it.
    pub files: u64,
    /// Their logical size.
    pub logical: ByteCount,
    /// Their allocated size.
    pub allocated: ByteCount,
    /// What removing everything below it would free now.
    pub reclaimable: ByteCount,
    /// Whether `reclaimable` is exact rather than a lower bound.
    pub exact: bool,
    /// What could and could not be read.
    pub coverage: RootCoverage,
}

/// What a whole scan found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanSummary {
    /// The scan.
    pub scan: ScanId,
    /// Each root.
    pub roots: Vec<RootSummary>,
    /// Entries seen.
    pub entries: u64,
    /// Directories listed.
    pub directories: u64,
    /// Entries skipped.
    pub skipped: u64,
    /// Why the scan stopped early, if it did.
    pub stopped: Option<StopReason>,
    /// The index database the scan was recorded in, if any.
    pub database: Option<PathBuf>,
}

/// Why a scan could not run or be recorded.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ScanError {
    /// A root could not be made absolute.
    #[error("{root}: {source}")]
    Root {
        /// The root as given.
        root: PathBuf,
        /// Why.
        source: std::io::Error,
    },
    /// A root is not a path the core can carry.
    #[error("{root}: {source}")]
    Path {
        /// The root as given.
        root: PathBuf,
        /// Why.
        source: PathError,
    },
    /// The index database failed.
    #[error("the index database failed: {0}")]
    Store(#[from] StoreError),
}

/// The platform's best enumerator: `getattrlistbulk` on macOS, which measures
/// clone-aware private sizes (ADR-0028).
#[cfg(target_os = "macos")]
fn enumerator() -> impl DirEnumerator {
    lumen_platform_macos::BulkEnumerator::new()
}

/// Directory handles on Windows (ADR-0017).
#[cfg(windows)]
fn enumerator() -> impl DirEnumerator {
    lumen_platform_windows::HandleEnumerator
}

#[cfg(all(unix, not(target_os = "macos")))]
fn enumerator() -> impl DirEnumerator {
    lumen_scan::StdFsEnumerator
}

/// `path` as the core carries it: raw bytes on Unix, UTF-16 on Windows.
///
/// # Errors
///
/// [`PathError`] if the path contains a NUL.
#[cfg(unix)]
pub fn raw_path(path: &Path) -> Result<RawPath, PathError> {
    use std::os::unix::ffi::OsStrExt as _;
    RawPath::from_unix_bytes(path.as_os_str().as_bytes().to_vec())
}

/// `path` as the core carries it: raw bytes on Unix, UTF-16 on Windows.
///
/// # Errors
///
/// [`PathError`] if the path contains a NUL.
#[cfg(windows)]
pub fn raw_path(path: &Path) -> Result<RawPath, PathError> {
    use std::os::windows::ffi::OsStrExt as _;
    RawPath::from_windows_wide(&path.as_os_str().encode_wide().collect::<Vec<_>>())
}

/// The roots as absolute raw paths, without resolving symlinks: a symlinked
/// root is reported as a link, never followed.
fn absolute_roots(roots: &[PathBuf]) -> Result<Vec<RawPath>, ScanError> {
    roots
        .iter()
        .map(|root| {
            let absolute = std::path::absolute(root).map_err(|source| ScanError::Root {
                root: root.clone(),
                source,
            })?;
            raw_path(&absolute).map_err(|source| ScanError::Path {
                root: root.clone(),
                source,
            })
        })
        .collect()
}

/// Scans `roots` read-only until done or `cancel` fires; with `db`, records
/// the scan and a snapshot of each root there.
///
/// # Errors
///
/// A [`ScanError`] if a root is unusable or the database fails. Unreadable
/// parts of the tree are not errors: they are reported in each root's
/// coverage, never treated as empty (ADR-0008).
pub fn scan(
    roots: &[PathBuf],
    db: Option<&Path>,
    clock: &dyn Clock,
    ids: &dyn IdGenerator,
    cancel: &CancelToken,
) -> Result<ScanSummary, ScanError> {
    let raw_roots = absolute_roots(roots)?;
    let scan_id = ids.scan_id();
    let started = clock.now();

    let writer = match db {
        Some(path) => Some(IndexWriter::start(
            IndexStore::new(open_index(path)?),
            scan_id,
            started,
            &raw_roots,
            WriterConfig::default(),
        )?),
        None => None,
    };

    let mut coverage = CoverageBuilder::new(&raw_roots);
    let mut sizes = SizeAggregator::new(&raw_roots);
    let mut write_error: Option<StoreError> = None;
    let summary = run_scan(
        &enumerator(),
        &raw_roots,
        &ScanConfig::default(),
        cancel,
        |item| {
            coverage.observe(&item);
            sizes.observe(&item);
            let stored = match (&writer, item) {
                (Some(w), ScanItem::Entry(entry)) => w.entry(entry),
                (Some(w), ScanItem::Listed { path, outcome }) => w.listing(path, outcome),
                _ => Ok(()),
            };
            match stored {
                Ok(()) => ControlFlow::Continue(()),
                Err(e) => {
                    write_error = Some(e);
                    ControlFlow::Break(())
                }
            }
        },
    );
    let coverage = coverage.finish(&summary);
    let totals = sizes.finish();

    let roots_summary: Vec<RootSummary> = coverage
        .roots
        .iter()
        .map(|entry| {
            let t = totals.get(&entry.root).copied().unwrap_or_default();
            RootSummary {
                root: entry.root.clone(),
                files: t.files,
                logical: t.logical,
                allocated: t.allocated,
                reclaimable: t.reclaimable_now,
                exact: t.exact,
                coverage: entry.coverage.clone(),
            }
        })
        .collect();

    if let Some(writer) = writer {
        let finished = clock.now();
        let state = match (&write_error, summary.stop) {
            (Some(_), _) => ScanState::Failed,
            (None, None) => ScanState::Completed,
            (None, Some(_)) => ScanState::Cancelled,
        };
        let (mut store, _stats) = writer.finish(state, finished)?;
        if let Some(e) = write_error {
            return Err(e.into());
        }
        let snapshots: Vec<RootSnapshot> = roots_summary
            .iter()
            .map(|r| RootSnapshot {
                root: r.root.clone(),
                taken_at: finished,
                files: r.files,
                logical: r.logical,
                allocated: r.allocated,
                reclaimable: r.reclaimable,
                exact: r.exact,
                complete: r.coverage.is_complete(),
            })
            .collect();
        store.record_snapshot(ids.snapshot_id(), scan_id, &snapshots)?;
    }

    Ok(ScanSummary {
        scan: scan_id,
        roots: roots_summary,
        entries: summary.totals.entries,
        directories: summary.totals.directories,
        skipped: summary.totals.skipped,
        stopped: summary.stop,
        database: db.map(Path::to_path_buf),
    })
}

#[cfg(test)]
mod tests {
    use lumen_testkit::{Fixture, Node};

    use super::*;
    use crate::{SystemClock, UuidV7Ids};

    fn fixture() -> Fixture {
        Fixture::build(&[
            Node::File {
                path: "a/one".into(),
                contents: vec![1; 1500],
            },
            Node::File {
                path: "a/b/two".into(),
                contents: vec![2; 2500],
            },
        ])
        .unwrap_or_else(|e| unreachable!("{e}"))
    }

    #[test]
    fn a_scan_counts_files_and_records_nothing_without_a_database() {
        let fx = fixture();
        let summary = scan(
            &[fx.root().to_path_buf()],
            None,
            &SystemClock,
            &UuidV7Ids,
            &CancelToken::new(),
        )
        .unwrap_or_else(|e| unreachable!("{e}"));
        let [root] = summary.roots.as_slice() else {
            unreachable!("one root")
        };
        assert_eq!(root.files, 2);
        assert_eq!(root.logical, ByteCount::new(4000));
        assert!(root.coverage.is_complete());
        assert_eq!((summary.stopped, summary.database), (None, None));
    }

    #[test]
    fn a_cancelled_scan_says_so() {
        let fx = fixture();
        let cancel = CancelToken::new();
        cancel.cancel();
        let summary = scan(
            &[fx.root().to_path_buf()],
            None,
            &SystemClock,
            &UuidV7Ids,
            &cancel,
        )
        .unwrap_or_else(|e| unreachable!("{e}"));
        assert_eq!(summary.stopped, Some(StopReason::Cancelled));
    }
}
