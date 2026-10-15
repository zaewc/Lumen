//! Runs a read-only scan and summarises it.

use std::error::Error;
use std::ops::ControlFlow;
use std::path::{Path, PathBuf};

use lumen_application::CancelToken;
use lumen_application::ports::{Clock, DirEnumerator, IdGenerator};
use lumen_domain::{ByteCount, DenialReason, PathError, RawPath, RootCoverage, ScanId, ScanState};
use lumen_scan::{
    CoverageBuilder, ScanConfig, ScanItem, SizeAggregator, StopReason, scan as run_scan,
};
use lumen_store_sqlite::{
    IndexStore, IndexWriter, RootSnapshot, StoreError, WriterConfig, open_index,
};
use serde_json::{Value, json};

/// Summary of one root.
#[derive(Debug)]
pub(crate) struct RootReport {
    root: RawPath,
    files: u64,
    logical: ByteCount,
    allocated: ByteCount,
    reclaimable: ByteCount,
    exact: bool,
    coverage: RootCoverage,
}

/// Summary of a whole scan.
#[derive(Debug)]
pub(crate) struct Report {
    scan: ScanId,
    roots: Vec<RootReport>,
    entries: u64,
    directories: u64,
    skipped: u64,
    stopped: Option<StopReason>,
    database: Option<PathBuf>,
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

#[cfg(unix)]
fn raw_path(path: &Path) -> Result<RawPath, PathError> {
    use std::os::unix::ffi::OsStrExt as _;
    RawPath::from_unix_bytes(path.as_os_str().as_bytes().to_vec())
}

#[cfg(windows)]
fn raw_path(path: &Path) -> Result<RawPath, PathError> {
    use std::os::windows::ffi::OsStrExt as _;
    RawPath::from_windows_wide(&path.as_os_str().encode_wide().collect::<Vec<_>>())
}

/// Scans `roots` read-only; with `db`, stores results and a snapshot there.
pub(crate) fn scan(
    roots: &[PathBuf],
    db: Option<&Path>,
    clock: &dyn Clock,
    ids: &dyn IdGenerator,
) -> Result<Report, Box<dyn Error>> {
    let raw_roots = roots
        .iter()
        .map(|root| {
            // Absolute without resolving symlinks: a symlinked root is reported as a
            // link, never followed.
            Ok(raw_path(&std::path::absolute(root)?)?)
        })
        .collect::<Result<Vec<_>, Box<dyn Error>>>()?;
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
        &CancelToken::new(),
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

    let roots_report: Vec<RootReport> = coverage
        .roots
        .iter()
        .map(|entry| {
            let t = totals.get(&entry.root).copied().unwrap_or_default();
            RootReport {
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
        let snapshots: Vec<RootSnapshot> = roots_report
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

    Ok(Report {
        scan: scan_id,
        roots: roots_report,
        entries: summary.totals.entries,
        directories: summary.totals.directories,
        skipped: summary.totals.skipped,
        stopped: summary.stop,
        database: db.map(Path::to_path_buf),
    })
}

/// Decimal units, as Finder and Explorer's "size" use.
fn human(bytes: ByteCount) -> String {
    const UNITS: [&str; 6] = ["B", "kB", "MB", "GB", "TB", "PB"];
    let mut value = bytes.get();
    let mut unit = 0;
    let mut remainder = 0;
    while value >= 1000 && unit + 1 < UNITS.len() {
        remainder = value % 1000;
        value /= 1000;
        unit += 1;
    }
    if unit == 0 {
        format!("{value} B")
    } else {
        format!("{value}.{} {}", remainder / 100, UNITS[unit])
    }
}

fn line(out: &mut String, text: &str) {
    out.push_str(text);
    out.push('\n');
}

fn denials(coverage: &RootCoverage) -> u64 {
    coverage.denied.values().sum()
}

fn failures(coverage: &RootCoverage) -> u64 {
    coverage.failed.values().sum()
}

impl Report {
    pub(crate) fn to_text(&self) -> String {
        let mut out = String::new();
        for r in &self.roots {
            line(&mut out, &r.root.display());
            line(&mut out, &format!("  files        {}", r.files));
            line(
                &mut out,
                &format!(
                    "  size         {} (allocated {})",
                    human(r.logical),
                    human(r.allocated)
                ),
            );
            let bound = if r.exact {
                ""
            } else {
                " (lower bound: shared blocks not measurable)"
            };
            line(
                &mut out,
                &format!("  reclaimable  {}{bound}", human(r.reclaimable)),
            );
            if r.coverage.is_complete() {
                line(&mut out, "  coverage     complete");
            } else {
                let tcc = r
                    .coverage
                    .denied
                    .get(&DenialReason::Tcc)
                    .copied()
                    .unwrap_or(0);
                let privacy = if tcc > 0 {
                    format!(" ({tcc} by privacy protection)")
                } else {
                    String::new()
                };
                line(
                    &mut out,
                    &format!(
                        "  coverage     INCOMPLETE: {} denied{privacy}, {} failed, {} not scanned; sizes above are lower bounds",
                        denials(&r.coverage),
                        failures(&r.coverage),
                        r.coverage.not_scanned,
                    ),
                );
            }
        }
        line(
            &mut out,
            &format!(
                "{} entries, {} directories, {} skipped",
                self.entries, self.directories, self.skipped
            ),
        );
        let partial = match self.stopped {
            Some(StopReason::Cancelled) => Some("scan was cancelled; results are partial"),
            Some(StopReason::Consumer) => Some("scan stopped early; results are partial"),
            None => None,
        };
        if let Some(message) = partial {
            line(&mut out, message);
        }
        if let Some(db) = &self.database {
            line(
                &mut out,
                &format!("saved scan {} to {}", self.scan, db.display()),
            );
        }
        line(&mut out, "nothing was modified");
        out
    }

    pub(crate) fn to_json(&self) -> Value {
        json!({
            "scan": self.scan,
            "roots": self.roots.iter().map(|r| json!({
                "root": r.root.display(),
                "files": r.files,
                "logical": r.logical,
                "allocated": r.allocated,
                "reclaimable_now": r.reclaimable,
                "reclaimable_exact": r.exact,
                "coverage_complete": r.coverage.is_complete(),
                "denied": denials(&r.coverage),
                "failed": failures(&r.coverage),
                "not_scanned": r.coverage.not_scanned,
            })).collect::<Vec<_>>(),
            "entries": self.entries,
            "directories": self.directories,
            "skipped": self.skipped,
            "partial": self.stopped.is_some(),
            "database": self.database.as_ref().map(|d| d.display().to_string()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn human_sizes_use_decimal_units_and_truncate() {
        assert_eq!(human(ByteCount::new(0)), "0 B");
        assert_eq!(human(ByteCount::new(999)), "999 B");
        assert_eq!(human(ByteCount::new(1_500)), "1.5 kB");
        assert_eq!(human(ByteCount::new(86_400_000_000)), "86.4 GB");
        assert_eq!(human(ByteCount::new(u64::MAX)), "18446.7 PB");
    }
}
