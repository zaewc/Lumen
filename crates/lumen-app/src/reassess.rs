//! Reassessing chosen items now (ADR-0058): the checks
//! `lumen_application::reassess` asks for, run on this machine.
//!
//! The scan's evidence and graph are loaded again with the inventories read
//! now, which answers who owns an item and whether that application is
//! installed. An item with an unread part or an old observation is listed
//! again from the filesystem, read-only and for at most [`CHECK_LIMIT`]; a
//! listing cut short leaves the rest unlisted, which the policy keeps. Every
//! item is asked live whether a process uses it ([`with_probes`], never the
//! assessment's snapshot). Nothing here changes the filesystem.

use std::ops::ControlFlow;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use lumen_application::CancelToken;
use lumen_application::assess::{Assessed, nested, select};
use lumen_application::ports::{Clock, DirEnumerator as _};
use lumen_application::reassess::{Gap, gaps};
pub use lumen_application::reassess::{MAX_ITEMS, Outcome, Reassessed, Reassessment, Verdicted};
use lumen_domain::{FilesystemEntry, RawPath};
use lumen_policy::PolicyError;
use lumen_scan::{ScanItem, scan as run_scan};

use crate::assess::{AssessError, Subtree, block_on, prepare, with_probes};
use crate::scan::{ScanError, enumerator, excluding_stores};

/// How long listing one item again may take.
pub const CHECK_LIMIT: Duration = Duration::from_secs(30);

/// Why a reassessment did not finish.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ReassessError {
    /// More items than one reassessment covers.
    #[error("{count} items chosen; reassess at most {MAX_ITEMS} at a time")]
    TooMany {
        /// The number chosen.
        count: usize,
    },
    /// The scan's evidence could not be loaded.
    #[error(transparent)]
    Assess(#[from] AssessError),
    /// The quarantine stores to leave out are unknown.
    #[error(transparent)]
    Scan(#[from] ScanError),
    /// The policy failed (a programming error).
    #[error(transparent)]
    Policy(#[from] PolicyError),
}

/// Reassesses `chosen`, items of an earlier assessment of the latest scan in
/// `db`, in order, checking `cancel` before each.
///
/// # Errors
///
/// [`ReassessError::TooMany`] above [`MAX_ITEMS`], before anything is read;
/// otherwise if the scan's evidence or the quarantine stores cannot be read.
pub fn reassess(
    db: &Path,
    chosen: &[Assessed],
    clock: &dyn Clock,
    cancel: &CancelToken,
) -> Result<Reassessment, ReassessError> {
    if chosen.len() > MAX_ITEMS {
        return Err(ReassessError::TooMany {
            count: chosen.len(),
        });
    }
    let assessment = prepare(db, clock)?;
    let candidates = select(&assessment.gathered, &assessment.entries);
    let stores = crate::store::scan_exclusion_paths().map_err(ScanError::from)?;
    with_probes(|probes| -> Result<Reassessment, ReassessError> {
        let mut items = Vec::with_capacity(chosen.len());
        for before in chosen {
            if cancel.is_cancelled() {
                return Ok(Reassessment {
                    items,
                    cancelled: true,
                });
            }
            let gaps = gaps(before);
            let path = &before.candidate.path;
            let inner = nested(&before.candidate, &candidates);
            let (subtree, current) = if gaps.is_empty() {
                // Settled: nothing is read.
                (Subtree::default(), None)
            } else if gaps.contains(&Gap::Unread) || gaps.contains(&Gap::Stale) {
                listed_again(path, &inner, &stores, cancel)?
            } else {
                (
                    assessment.subtree_without(path, &inner)?,
                    enumerator().stat(path).ok(),
                )
            };
            let sources = assessment.sources(&subtree);
            items.push(block_on(lumen_application::reassess::reassess(
                before,
                current.as_ref(),
                &sources,
                probes,
                clock,
            ))?);
        }
        Ok(Reassessment {
            items,
            cancelled: false,
        })
    })?
}

/// Everything at and below `path` except `nested`, listed now without
/// following links and without entering quarantine stores, and the item
/// itself as observed now (`None` if it is gone). After [`CHECK_LIMIT`] the
/// listing stops; what was not listed has no listing, so the item stays kept
/// as not read completely.
fn listed_again(
    path: &RawPath,
    nested: &[&RawPath],
    stores: &[PathBuf],
    cancel: &CancelToken,
) -> Result<(Subtree, Option<FilesystemEntry>), ReassessError> {
    let mut entries: Vec<FilesystemEntry> = Vec::new();
    let mut listings = Vec::new();
    let Some(native) = crate::space_change::native(path) else {
        return Ok((Subtree::default(), None));
    };
    let config = excluding_stores(&[native], stores)?;
    let started = Instant::now();
    run_scan(
        &enumerator(),
        std::slice::from_ref(path),
        &config,
        cancel,
        |item| {
            if started.elapsed() > CHECK_LIMIT {
                return ControlFlow::Break(());
            }
            match item {
                ScanItem::Entry(entry) => entries.push(entry),
                ScanItem::Listed { path, outcome } => listings.push((path, outcome)),
                _ => {}
            }
            ControlFlow::Continue(())
        },
    );
    let current = entries.iter().find(|entry| entry.path == *path).cloned();
    Ok((Subtree::listed(entries, listings, nested), current))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_scan_is_an_error_not_an_empty_result() {
        let missing = Path::new("/nonexistent/lumen/index.db");
        assert!(matches!(
            reassess(missing, &[], &crate::SystemClock, &CancelToken::new()),
            Err(ReassessError::Assess(_))
        ));
    }
}
