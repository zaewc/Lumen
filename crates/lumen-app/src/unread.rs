//! The folders a scan could not read in full, by path and why (ADR-0008).
//!
//! What was not read is unknown, never empty: these folders are left as they
//! are, and every size above them is a lower bound. Listing them lets a person
//! see where the blind spots are and, where they can, do something about it
//! (grant access, scan again). The paths stay on this computer: they are for
//! the person, never sent to a model.
//!
//! A refused folder has nothing below it in the index, so the list holds no
//! folder inside another refused one; a folder cut short can hold unread
//! folders of its own, and each is listed with its own reason.

use std::path::Path;

use lumen_application::ports::DirOutcome;
use lumen_domain::{AccessState, DenialReason, FailureKind, RawPath, ScanId};
use lumen_store_sqlite::{IndexStore, StoreError, open_index};

/// Why a folder was not read in full.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Unread {
    /// Access was refused.
    Denied(DenialReason),
    /// Reading it failed.
    Failed(FailureKind),
    /// It was read, but not every entry was delivered.
    CutShort,
    /// It was never entered: a mount point, an excluded folder, or one the
    /// scan did not reach.
    NotEntered,
}

impl Unread {
    /// How reading a folder ended, as stored; `None` when it was never listed.
    fn from_outcome(outcome: Option<DirOutcome>) -> Self {
        match outcome.map(|o| o.access) {
            Some(AccessState::Denied(reason)) => Self::Denied(reason),
            Some(AccessState::Failed(kind)) => Self::Failed(kind),
            // Only an incomplete readable listing is stored as unread.
            Some(AccessState::Readable) => Self::CutShort,
            Some(AccessState::NotScanned) | None => Self::NotEntered,
        }
    }
}

/// One folder not read in full.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnreadFolder {
    /// The folder.
    pub path: RawPath,
    /// Why.
    pub why: Unread,
}

/// The folders a scan did not read in full.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnreadFolders {
    /// The scan.
    pub scan: ScanId,
    /// How many there are, including those beyond `folders`.
    pub total: u64,
    /// The first of them, in path order, at most the limit asked for.
    pub folders: Vec<UnreadFolder>,
}

/// Why the unread folders could not be listed.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum UnreadError {
    /// The database has no such scan.
    #[error("this scan is not in the database")]
    NoSuchScan,
    /// The index database failed.
    #[error("the index database failed: {0}")]
    Store(#[from] StoreError),
}

/// The folders `scan` did not read in full, at most `limit` of them, with
/// the total. Reads only the index; touches no file.
///
/// # Errors
///
/// [`UnreadError::NoSuchScan`] for a scan the database does not hold, or
/// [`UnreadError::Store`] if the index fails.
pub fn unread_folders(db: &Path, scan: ScanId, limit: usize) -> Result<UnreadFolders, UnreadError> {
    let store = IndexStore::new(open_index(db)?);
    if store.scan_state(scan)?.is_none() {
        return Err(UnreadError::NoSuchScan);
    }
    Ok(collect(scan, store.unread_directories(scan)?, limit))
}

fn collect(
    scan: ScanId,
    unread: Vec<(RawPath, Option<DirOutcome>)>,
    limit: usize,
) -> UnreadFolders {
    let total = u64::try_from(unread.len()).unwrap_or(u64::MAX);
    let folders = unread
        .into_iter()
        .take(limit)
        .map(|(path, outcome)| UnreadFolder {
            path,
            why: Unread::from_outcome(outcome),
        })
        .collect();
    UnreadFolders {
        scan,
        total,
        folders,
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;

    use lumen_domain::{
        ByteCount, EntryKind, EntryTimes, FileId, FileIdentity, FilesystemEntry, Protection,
        ScanState, SizeFacts, SizeFlags, Timestamp, VolumeId,
    };

    use super::*;

    fn raw(path: &str) -> RawPath {
        RawPath::from_unix_bytes(path.as_bytes().to_vec()).unwrap_or_else(|e| unreachable!("{e}"))
    }

    fn dir(path: &str, file: u128) -> FilesystemEntry {
        FilesystemEntry {
            path: raw(path),
            kind: EntryKind::Directory,
            size: SizeFacts {
                identity: FileIdentity {
                    volume: VolumeId::new("v").unwrap_or_else(|e| unreachable!("{e}")),
                    file: FileId::new(file),
                },
                logical: ByteCount::ZERO,
                allocated: ByteCount::ZERO,
                private: None,
                clone_id: None,
                link_count: NonZeroU32::MIN,
                flags: SizeFlags::default(),
            },
            times: EntryTimes::default(),
            protection: Protection::default(),
            access: AccessState::Readable,
        }
    }

    fn outcome(access: AccessState, complete: bool) -> DirOutcome {
        DirOutcome { access, complete }
    }

    #[test]
    fn each_unread_folder_is_listed_with_why() -> Result<(), Box<dyn std::error::Error>> {
        let fixture = lumen_testkit::Fixture::build(&[])?;
        let db = fixture.root().join("index.db");
        let mut store = IndexStore::new(open_index(&db)?);
        let scan: ScanId = "0190f0d2-6c1e-7a3b-8c4d-5e6f708192a7".parse()?;
        store.begin_scan(scan, Timestamp::UNIX_EPOCH, &[raw("/h")])?;
        store.insert_entries(
            scan,
            &[
                dir("/h", 1),
                dir("/h/fine", 2),
                dir("/h/mail", 3),
                dir("/h/slow", 4),
                dir("/h/half", 5),
                dir("/h/mnt", 6),
            ],
        )?;
        store.insert_listings(
            scan,
            &[
                (raw("/h"), outcome(AccessState::Readable, true)),
                (raw("/h/fine"), outcome(AccessState::Readable, true)),
                (
                    raw("/h/mail"),
                    outcome(AccessState::Denied(DenialReason::Tcc), false),
                ),
                (
                    raw("/h/slow"),
                    outcome(AccessState::Failed(FailureKind::TimedOut), false),
                ),
                (raw("/h/half"), outcome(AccessState::Readable, false)),
            ],
        )?;
        store.finish_scan(scan, ScanState::Completed, Timestamp::UNIX_EPOCH)?;
        drop(store);

        let all = unread_folders(&db, scan, 100)?;
        let listed: Vec<_> = all
            .folders
            .iter()
            .map(|f| (f.path.clone(), f.why))
            .collect();
        assert_eq!(
            listed,
            [
                (raw("/h/half"), Unread::CutShort),
                (raw("/h/mail"), Unread::Denied(DenialReason::Tcc)),
                (raw("/h/mnt"), Unread::NotEntered),
                (raw("/h/slow"), Unread::Failed(FailureKind::TimedOut)),
            ],
            "a fully read folder is never listed"
        );
        assert_eq!(all.total, 4);

        let first = unread_folders(&db, scan, 1)?;
        assert_eq!(first.folders.len(), 1);
        assert_eq!(first.total, 4, "the total counts those beyond the limit");

        let other: ScanId = "0190f0d2-6c1e-7a3b-8c4d-5e6f708192a8".parse()?;
        assert!(matches!(
            unread_folders(&db, other, 100),
            Err(UnreadError::NoSuchScan)
        ));
        Ok(())
    }

    #[test]
    fn never_listed_and_not_scanned_both_mean_not_entered() {
        assert_eq!(Unread::from_outcome(None), Unread::NotEntered);
        assert_eq!(
            Unread::from_outcome(Some(outcome(AccessState::NotScanned, false))),
            Unread::NotEntered
        );
    }
}
