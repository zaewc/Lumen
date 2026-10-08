//! Turns scan results into a [`CoverageReport`] (ADR-0008).
//!
//! Every directory outcome is attributed to the scan root it belongs to. The
//! mapping is deliberately conservative: anything short of a complete, readable
//! listing makes the root incomplete, and a traversal that stopped early marks
//! every root incomplete, so a report can never claim coverage it does not have.

use lumen_application::ports::{DirOutcome, EnumerateError};
use lumen_domain::{AccessState, CoverageReport, FailureKind, RawPath, RootCoverage, RootEntry};

use crate::{ScanItem, ScanSummary, SkipReason};

/// Accumulates per-root coverage from [`ScanItem`]s.
#[derive(Debug, Clone)]
pub struct CoverageBuilder {
    roots: Vec<RootEntry>,
}

impl CoverageBuilder {
    /// Starts a report for the given scan roots.
    pub fn new(roots: &[RawPath]) -> Self {
        Self {
            roots: roots
                .iter()
                .map(|root| RootEntry {
                    root: root.clone(),
                    coverage: RootCoverage::default(),
                })
                .collect(),
        }
    }

    /// Records one scan result. Entries and progress snapshots carry no coverage
    /// information and are ignored.
    pub fn observe(&mut self, item: &ScanItem) {
        let (path, state) = match item {
            ScanItem::Listed { path, outcome } => (path, listing_state(*outcome)),
            ScanItem::Skipped { path, reason } => match skip_state(reason) {
                Some(state) => (path, state),
                None => return,
            },
            // Listed explicitly so a new result kind must decide how it affects coverage.
            ScanItem::Entry(_) | ScanItem::Progress(_) => return,
        };
        if let Some(entry) = self.root_for(path) {
            entry.coverage.record(state);
        }
    }

    /// Finishes the report. If the traversal stopped early, every root is marked
    /// incomplete: directories that were never reached are unknown.
    pub fn finish(mut self, summary: &ScanSummary) -> CoverageReport {
        if summary.stop.is_some() {
            for entry in &mut self.roots {
                entry.coverage.record(AccessState::NotScanned);
            }
        }
        CoverageReport {
            roots: self.roots,
            ..CoverageReport::default()
        }
    }

    /// The most specific root containing `path` (whole path segments only).
    fn root_for(&mut self, path: &RawPath) -> Option<&mut RootEntry> {
        self.roots
            .iter_mut()
            .filter(|entry| {
                is_within(entry.root.as_bytes(), path.as_bytes())
                    && entry.root.flavor() == path.flavor()
            })
            .max_by_key(|entry| entry.root.as_bytes().len())
    }
}

/// Whether `path` is `root` or lies under it, comparing whole segments.
fn is_within(root: &[u8], path: &[u8]) -> bool {
    let Some(rest) = path.strip_prefix(root) else {
        return false;
    };
    rest.is_empty()
        || root.ends_with(b"/")
        || root.ends_with(b"\\")
        || rest.starts_with(b"/")
        || rest.starts_with(b"\\")
}

fn listing_state(outcome: DirOutcome) -> AccessState {
    match (outcome.access, outcome.complete) {
        (AccessState::Readable, true) => AccessState::Readable,
        // Readable but not every entry was delivered: never count it as complete.
        (AccessState::Readable, false) => AccessState::Failed(FailureKind::Io),
        (other, _) => other,
    }
}

fn skip_state(reason: &SkipReason) -> Option<AccessState> {
    match reason {
        SkipReason::MountPoint => Some(AccessState::NotScanned),
        SkipReason::Enumerate(EnumerateError::Unobservable { access, .. }) => Some(*access),
        // A root that is a plain file has no directories to cover.
        SkipReason::Enumerate(EnumerateError::NotADirectory { .. }) => None,
        // Identity changed, unsupported path, or a future error kind: unknown.
        SkipReason::Enumerate(_) => Some(AccessState::Failed(FailureKind::Io)),
    }
}

#[cfg(test)]
mod tests {
    use lumen_domain::{DenialReason, FileId, FileIdentity, VolumeId};

    use super::*;
    use crate::{ScanProgress, StopReason};

    fn path(p: &str) -> RawPath {
        RawPath::from_unix_bytes(p.as_bytes().to_vec()).unwrap_or_else(|_| unreachable!())
    }

    fn listed(p: &str, access: AccessState, complete: bool) -> ScanItem {
        ScanItem::Listed {
            path: path(p),
            outcome: DirOutcome { access, complete },
        }
    }

    fn summary(stop: Option<StopReason>) -> ScanSummary {
        ScanSummary {
            totals: ScanProgress::default(),
            stop,
        }
    }

    fn coverage_of<'a>(report: &'a CoverageReport, root: &str) -> Option<&'a RootCoverage> {
        report
            .roots
            .iter()
            .find(|e| e.root == path(root))
            .map(|e| &e.coverage)
    }

    #[test]
    fn complete_readable_listings_give_complete_coverage() {
        let mut b = CoverageBuilder::new(&[path("/r")]);
        b.observe(&listed("/r", AccessState::Readable, true));
        b.observe(&listed("/r/a", AccessState::Readable, true));
        let report = b.finish(&summary(None));
        assert!(report.is_complete());
        assert_eq!(coverage_of(&report, "/r").map(|c| c.readable), Some(2));
    }

    #[test]
    fn denied_failed_and_incomplete_listings_make_the_root_incomplete() {
        for item in [
            listed("/r/x", AccessState::Denied(DenialReason::Tcc), false),
            listed("/r/x", AccessState::Failed(FailureKind::TimedOut), false),
            listed("/r/x", AccessState::Readable, false),
        ] {
            let mut b = CoverageBuilder::new(&[path("/r")]);
            b.observe(&listed("/r", AccessState::Readable, true));
            b.observe(&item);
            assert!(!b.finish(&summary(None)).is_complete(), "{item:?}");
        }
    }

    #[test]
    fn mount_points_and_unobservable_paths_are_recorded() {
        let mut b = CoverageBuilder::new(&[path("/r")]);
        b.observe(&ScanItem::Skipped {
            path: path("/r/mnt"),
            reason: SkipReason::MountPoint,
        });
        b.observe(&ScanItem::Skipped {
            path: path("/r/gone"),
            reason: SkipReason::Enumerate(EnumerateError::Unobservable {
                path: path("/r/gone"),
                access: AccessState::Failed(FailureKind::Vanished),
            }),
        });
        b.observe(&ScanItem::Skipped {
            path: path("/r/swapped"),
            reason: SkipReason::Enumerate(EnumerateError::IdentityChanged {
                path: path("/r/swapped"),
                expected: FileIdentity {
                    volume: VolumeId::new("v").unwrap_or_else(|_| unreachable!()),
                    file: FileId::new(1),
                },
            }),
        });
        let report = b.finish(&summary(None));
        let root = coverage_of(&report, "/r");
        assert_eq!(root.map(|c| c.not_scanned), Some(1));
        assert_eq!(
            root.and_then(|c| c.failed.get(&FailureKind::Vanished)),
            Some(&1)
        );
        assert_eq!(root.and_then(|c| c.failed.get(&FailureKind::Io)), Some(&1));
    }

    #[test]
    fn stopping_early_marks_every_root_incomplete() {
        for stop in [StopReason::Cancelled, StopReason::Consumer] {
            let mut b = CoverageBuilder::new(&[path("/a"), path("/b")]);
            b.observe(&listed("/a", AccessState::Readable, true));
            let report = b.finish(&summary(Some(stop)));
            assert!(
                report.roots.iter().all(|e| !e.coverage.is_complete()),
                "{stop:?}"
            );
        }
    }

    #[test]
    fn outcomes_go_to_the_most_specific_root_by_whole_segments() {
        let mut b = CoverageBuilder::new(&[path("/r"), path("/r/inner"), path("/r2")]);
        b.observe(&listed(
            "/r/inner/x",
            AccessState::Denied(DenialReason::Posix),
            false,
        ));
        b.observe(&listed("/r2/y", AccessState::Readable, true));
        b.observe(&listed("/r/other", AccessState::Readable, true));
        let report = b.finish(&summary(None));
        assert_eq!(
            coverage_of(&report, "/r/inner").map(RootCoverage::is_complete),
            Some(false)
        );
        assert_eq!(
            coverage_of(&report, "/r").map(|c| (c.readable, c.is_complete())),
            Some((1, true))
        );
        assert_eq!(
            coverage_of(&report, "/r2").map(|c| c.readable),
            Some(1),
            "/r2 is not under /r"
        );
    }

    #[test]
    fn file_roots_have_nothing_to_cover() {
        let mut b = CoverageBuilder::new(&[path("/f")]);
        b.observe(&ScanItem::Skipped {
            path: path("/f"),
            reason: SkipReason::Enumerate(EnumerateError::NotADirectory { path: path("/f") }),
        });
        assert!(b.finish(&summary(None)).is_complete());
    }

    #[test]
    fn end_to_end_with_the_scheduler() {
        use std::ops::ControlFlow;

        use lumen_application::CancelToken;

        use crate::{ScanConfig, scan};

        struct OneDenied;
        impl lumen_application::ports::DirEnumerator for OneDenied {
            fn stat(&self, p: &RawPath) -> Result<lumen_domain::FilesystemEntry, EnumerateError> {
                Err(EnumerateError::Unobservable {
                    path: p.clone(),
                    access: AccessState::Denied(DenialReason::Tcc),
                })
            }
            fn read_dir(
                &self,
                _p: &RawPath,
                _e: Option<&FileIdentity>,
                _s: &mut dyn FnMut(lumen_domain::FilesystemEntry) -> ControlFlow<()>,
            ) -> Result<DirOutcome, EnumerateError> {
                Ok(DirOutcome {
                    access: AccessState::Readable,
                    complete: true,
                })
            }
        }
        let roots = [path("/Users/ana/Library/Mail")];
        let mut builder = CoverageBuilder::new(&roots);
        let summary = scan(
            &OneDenied,
            &roots,
            &ScanConfig::default(),
            &CancelToken::new(),
            |item| {
                builder.observe(&item);
                ControlFlow::Continue(())
            },
        );
        let report = builder.finish(&summary);
        assert!(
            !report.is_complete(),
            "a TCC-denied root is a blind spot, never an empty folder"
        );
        assert_eq!(
            coverage_of(&report, "/Users/ana/Library/Mail")
                .and_then(|c| c.denied.get(&DenialReason::Tcc)),
            Some(&1)
        );
    }
}
