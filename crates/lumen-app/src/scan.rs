//! Read-only scans of chosen roots (roadmap 5.x, 11.2): the platform's best
//! enumerator, coverage and size aggregation, and, with a database, the
//! recorded scan and a snapshot of each root.

use std::collections::BTreeMap;
use std::ops::ControlFlow;
use std::path::{Path, PathBuf};

use crate::space::{self, SpaceError};
use lumen_application::CancelToken;
use lumen_application::ports::{Clock, DirEnumerator, IdGenerator};
use lumen_domain::{
    AccessState, ByteCount, CoverageReport, EntryKind, PathError, RawPath, RootCoverage, ScanId,
    ScanState, Timestamp,
};
use lumen_scan::{
    CoverageBuilder, DirTotals, ScanConfig, ScanItem, ScanProgress, SizeAggregator, SkipReason,
    StopReason, scan as run_scan,
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
    /// Whether recording stopped because the disk holding the database was
    /// nearly full ([`crate::space::RESERVE`]); the results are partial.
    pub out_of_space: bool,
}

/// What a running scan reports as it goes, for a live view on the user's own
/// screen. Telemetry never sees these paths (ADR-0022).
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ScanEvent {
    /// The scan is starting on these roots.
    Started {
        /// The roots, absolute.
        roots: Vec<RawPath>,
    },
    /// Totals so far.
    Progress {
        /// Entries, directories, skips and logical file bytes so far.
        totals: ScanProgress,
        /// Disk space the files seen so far take up (sparse and compressed
        /// files take less than their logical size; hard links count once
        /// per path).
        allocated: ByteCount,
        /// The directory listed most recently.
        current: Option<RawPath>,
    },
    /// A directory whose contents could not be read: unknown, not empty
    /// (ADR-0008).
    Unreadable {
        /// The directory.
        path: RawPath,
        /// Why.
        access: AccessState,
    },
    /// A directory that was not entered.
    Skipped {
        /// The directory.
        path: RawPath,
        /// Why.
        reason: SkipReason,
    },
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
    /// Recording would leave too little free space, or it cannot be measured.
    #[error(transparent)]
    Space(#[from] SpaceError),
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
/// the scan and a snapshot of each root there. `on_event` sees the scan as it
/// goes. Recording does not start, and stops early, when the disk holding
/// `db` would have less than [`space::RESERVE`] free; the summary then says
/// [`ScanSummary::out_of_space`].
///
/// # Errors
///
/// A [`ScanError`] if a root is unusable, the database fails, or there is no
/// room to start recording. Unreadable
/// parts of the tree are not errors: they are reported in each root's
/// coverage, never treated as empty (ADR-0008).
#[tracing::instrument(name = "scan", skip_all, fields(roots = roots.len()))]
pub fn scan(
    roots: &[PathBuf],
    db: Option<&Path>,
    clock: &dyn Clock,
    ids: &dyn IdGenerator,
    cancel: &CancelToken,
    on_event: &mut dyn FnMut(&ScanEvent),
) -> Result<ScanSummary, ScanError> {
    let guard = |path: &Path| space::ensure(path, 0);
    guarded(roots, db, clock, ids, cancel, on_event, &guard)
}

/// [`scan`], with `room` deciding whether the database may keep growing:
/// asked before recording starts and at every progress snapshot.
fn guarded(
    roots: &[PathBuf],
    db: Option<&Path>,
    clock: &dyn Clock,
    ids: &dyn IdGenerator,
    cancel: &CancelToken,
    on_event: &mut dyn FnMut(&ScanEvent),
    room: &dyn Fn(&Path) -> Result<(), SpaceError>,
) -> Result<ScanSummary, ScanError> {
    let timer = std::time::Instant::now();
    let raw_roots = absolute_roots(roots)?;
    on_event(&ScanEvent::Started {
        roots: raw_roots.clone(),
    });
    let scan_id = ids.scan_id();
    let started = clock.now();

    if let Some(path) = db {
        room(path)?;
    }
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
    let mut live = Live::default();
    let mut out_of_space = false;
    let summary = run_scan(
        &enumerator(),
        &raw_roots,
        &ScanConfig::default(),
        cancel,
        |item| {
            coverage.observe(&item);
            sizes.observe(&item);
            live.report(&item, on_event);
            if let (Some(path), ScanItem::Progress(_)) = (db, &item)
                && room(path).is_err()
            {
                out_of_space = true;
                return ControlFlow::Break(());
            }
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

    let roots_summary = root_summaries(&coverage, &totals);

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
        store.record_snapshot(
            ids.snapshot_id(),
            scan_id,
            &snapshots(&roots_summary, finished),
        )?;
    }

    let result = ScanSummary {
        scan: scan_id,
        roots: roots_summary,
        entries: summary.totals.entries,
        directories: summary.totals.directories,
        skipped: summary.totals.skipped,
        stopped: summary.stop,
        database: db.map(Path::to_path_buf),
        out_of_space,
    };
    finished(&result, timer.elapsed());
    Ok(result)
}

/// Each root's sizes and coverage.
fn root_summaries(
    coverage: &CoverageReport,
    totals: &BTreeMap<RawPath, DirTotals>,
) -> Vec<RootSummary> {
    coverage
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
        .collect()
}

/// Each root's snapshot, taken at `at`.
fn snapshots(roots: &[RootSummary], at: Timestamp) -> Vec<RootSnapshot> {
    roots
        .iter()
        .map(|r| RootSnapshot {
            root: r.root.clone(),
            taken_at: at,
            files: r.files,
            logical: r.logical,
            allocated: r.allocated,
            reclaimable: r.reclaimable,
            exact: r.exact,
            complete: r.coverage.is_complete(),
        })
        .collect()
}

/// What the live view remembers between results.
#[derive(Default)]
struct Live {
    /// The directory listed most recently.
    current: Option<RawPath>,
    /// Allocated bytes of the files seen so far.
    allocated: ByteCount,
}

impl Live {
    /// Turns one scan result into a live event, if it is one the user would
    /// want to see.
    fn report(&mut self, item: &ScanItem, on_event: &mut dyn FnMut(&ScanEvent)) {
        match item {
            ScanItem::Entry(entry) if entry.kind == EntryKind::File => {
                self.allocated = self.allocated.saturating_add(entry.size.allocated);
            }
            ScanItem::Listed { path, outcome } if !outcome.access.is_observed() => {
                on_event(&ScanEvent::Unreadable {
                    path: path.clone(),
                    access: outcome.access,
                });
            }
            ScanItem::Listed { path, .. } => self.current = Some(path.clone()),
            ScanItem::Skipped { path, reason } => on_event(&ScanEvent::Skipped {
                path: path.clone(),
                reason: reason.clone(),
            }),
            ScanItem::Progress(totals) => on_event(&ScanEvent::Progress {
                totals: *totals,
                allocated: self.allocated,
                current: self.current.clone(),
            }),
            _ => {}
        }
    }
}

/// The scan's summary event: counts and timing only, never paths (ADR-0022).
fn finished(summary: &ScanSummary, elapsed: std::time::Duration) {
    tracing::info!(
        entries = summary.entries,
        directories = summary.directories,
        skipped = summary.skipped,
        files = summary.roots.iter().map(|r| r.files).sum::<u64>(),
        complete_roots = summary
            .roots
            .iter()
            .filter(|r| r.coverage.is_complete())
            .count(),
        elapsed_ms = u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX),
        stopped_early = summary.stopped.is_some(),
        recorded = summary.database.is_some(),
        "scan finished"
    );
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
            &mut |_| {},
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
            &mut |_| {},
        )
        .unwrap_or_else(|e| unreachable!("{e}"));
        assert_eq!(summary.stopped, Some(StopReason::Cancelled));
    }

    #[test]
    fn recording_does_not_start_without_room() {
        let fx = fixture();
        let db = fx.path("index.db").unwrap_or_else(|e| unreachable!("{e}"));
        let full = |_: &Path| space::check(0, 0);
        let result = guarded(
            &[fx.root().to_path_buf()],
            Some(&db),
            &SystemClock,
            &UuidV7Ids,
            &CancelToken::new(),
            &mut |_| {},
            &full,
        );
        assert!(matches!(result, Err(ScanError::Space(_))), "{result:?}");
        assert!(!db.exists(), "nothing was written");
    }

    #[test]
    fn recording_stops_when_the_disk_fills_and_says_so() {
        let fx = Fixture::build(
            &(0..2500)
                .map(|i| Node::File {
                    path: format!("many/{i}"),
                    contents: Vec::new(),
                })
                .collect::<Vec<_>>(),
        )
        .unwrap_or_else(|e| unreachable!("{e}"));
        let db = fx.path("index.db").unwrap_or_else(|e| unreachable!("{e}"));
        // Room at the start, none from the first progress snapshot on.
        let asked = std::cell::Cell::new(0);
        let filling = |_: &Path| {
            asked.set(asked.get() + 1);
            space::check(if asked.get() == 1 { u64::MAX } else { 0 }, 0)
        };
        let summary = guarded(
            &[fx.root().to_path_buf()],
            Some(&db),
            &SystemClock,
            &UuidV7Ids,
            &CancelToken::new(),
            &mut |_| {},
            &filling,
        )
        .unwrap_or_else(|e| unreachable!("{e}"));
        assert!(summary.out_of_space);
        assert_eq!(summary.stopped, Some(StopReason::Consumer));
        assert!(summary.entries < 2500, "stopped early: {}", summary.entries);
        assert!(
            summary.roots.iter().all(|r| !r.coverage.is_complete()),
            "a stopped scan is incomplete, not smaller"
        );
    }

    #[test]
    fn the_scan_reports_its_start_and_final_totals_as_it_goes() {
        let fx = fixture();
        let mut events = Vec::new();
        let summary = scan(
            &[fx.root().to_path_buf()],
            None,
            &SystemClock,
            &UuidV7Ids,
            &CancelToken::new(),
            &mut |event| events.push(event.clone()),
        )
        .unwrap_or_else(|e| unreachable!("{e}"));
        let Some(ScanEvent::Started { roots }) = events.first() else {
            unreachable!("the first event is the start: {events:?}")
        };
        assert_eq!(roots.len(), 1);
        let Some(ScanEvent::Progress {
            totals,
            allocated,
            current,
        }) = events.last()
        else {
            unreachable!("the last event is the final totals: {events:?}")
        };
        assert_eq!(totals.entries, summary.entries);
        assert_eq!(totals.file_bytes, ByteCount::new(4000));
        let [root] = summary.roots.as_slice() else {
            unreachable!("one root")
        };
        assert_eq!(*allocated, root.allocated);
        assert!(current.is_some(), "a directory was listed");
    }
}
