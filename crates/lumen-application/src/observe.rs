//! What the scan saw of one subject, for the policy (roadmap 10.7).
//!
//! A file is complete when its metadata was read, and removing it frees what
//! its size facts say. A directory is complete only when it and every
//! directory below it were listed in full (a directory without a listing, or
//! with a refused or partial one, makes it incomplete: unread is never treated
//! as empty, ADR-0008); removing it frees what everything under it frees
//! together, hard links and clones accounted for by
//! [`ReclaimEstimate::for_selection`].

use std::collections::HashMap;

use lumen_domain::{EntryKind, FilesystemEntry, PathFlavor, RawPath, ReclaimEstimate};
use lumen_policy::Observed;

use crate::ports::DirOutcome;

/// Whether `path` lies strictly below the directory `dir` (same flavor).
fn below(path: &RawPath, dir: &RawPath) -> bool {
    if path.flavor() != dir.flavor() {
        return false;
    }
    let separator = match dir.flavor() {
        PathFlavor::Unix => b'/',
        PathFlavor::Windows => b'\\',
    };
    let dir = dir.as_bytes();
    let dir = dir.strip_suffix(&[separator]).unwrap_or(dir);
    path.as_bytes()
        .strip_prefix(dir)
        .and_then(|rest| rest.strip_prefix(&[separator]))
        .is_some_and(|rest| !rest.is_empty())
}

fn fully_listed(listings: &HashMap<&RawPath, DirOutcome>, dir: &RawPath) -> bool {
    listings
        .get(dir)
        .is_some_and(|outcome| outcome.access.is_observed() && outcome.complete)
}

/// What the scan saw of `subject`, given everything it stored (`scanned`,
/// `listings`).
pub fn observed(
    subject: &FilesystemEntry,
    scanned: &[FilesystemEntry],
    listings: &[(RawPath, DirOutcome)],
) -> Observed {
    if subject.kind != EntryKind::Directory {
        return Observed {
            entry: subject.clone(),
            complete: subject.access.is_observed(),
            reclaim: ReclaimEstimate::for_selection([&subject.size]),
        };
    }
    let listings: HashMap<&RawPath, DirOutcome> = listings
        .iter()
        .map(|(path, outcome)| (path, *outcome))
        .collect();
    let descendants: Vec<&FilesystemEntry> = scanned
        .iter()
        .filter(|entry| below(&entry.path, &subject.path))
        .collect();
    let complete = subject.access.is_observed()
        && fully_listed(&listings, &subject.path)
        && descendants
            .iter()
            .filter(|entry| entry.kind == EntryKind::Directory)
            .all(|dir| dir.access.is_observed() && fully_listed(&listings, &dir.path));
    let reclaim = ReclaimEstimate::for_selection(
        std::iter::once(&subject.size).chain(descendants.iter().map(|entry| &entry.size)),
    );
    Observed {
        entry: subject.clone(),
        complete,
        reclaim,
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;

    use lumen_domain::{
        AccessState, ByteCount, DenialReason, EntryTimes, FileId, FileIdentity, Protection,
        SizeFacts, SizeFlags, VolumeId,
    };

    use super::*;

    fn entry(path: &str, kind: EntryKind, file: u128, allocated: u64) -> FilesystemEntry {
        FilesystemEntry {
            path: RawPath::from_unix_bytes(path.as_bytes().to_vec())
                .unwrap_or_else(|_| unreachable!()),
            kind,
            size: SizeFacts {
                identity: FileIdentity {
                    volume: VolumeId::new("v").unwrap_or_else(|_| unreachable!()),
                    file: FileId::new(file),
                },
                logical: ByteCount::new(allocated),
                allocated: ByteCount::new(allocated),
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

    fn listed(path: &str, access: AccessState, complete: bool) -> (RawPath, DirOutcome) {
        (
            RawPath::from_unix_bytes(path.as_bytes().to_vec()).unwrap_or_else(|_| unreachable!()),
            DirOutcome { access, complete },
        )
    }

    fn tree() -> Vec<FilesystemEntry> {
        vec![
            entry("/c", EntryKind::Directory, 1, 0),
            entry("/c/a", EntryKind::File, 2, 4096),
            entry("/c/sub", EntryKind::Directory, 3, 0),
            entry("/c/sub/b", EntryKind::File, 4, 8192),
            entry("/cache-sibling", EntryKind::File, 5, 1_000_000),
        ]
    }

    #[test]
    fn a_fully_listed_directory_is_complete_and_frees_its_whole_subtree() {
        let scanned = tree();
        let listings = [
            listed("/c", AccessState::Readable, true),
            listed("/c/sub", AccessState::Readable, true),
        ];
        let observed = observed(&scanned[0], &scanned, &listings);
        assert!(observed.complete);
        assert_eq!(
            observed.reclaim,
            ReclaimEstimate::for_selection(scanned[..4].iter().map(|e| &e.size)),
            "the sibling with a shared prefix is not included"
        );
    }

    #[test]
    fn an_unlisted_or_refused_directory_below_makes_it_incomplete() {
        let scanned = tree();
        for listings in [
            vec![listed("/c", AccessState::Readable, true)],
            vec![
                listed("/c", AccessState::Readable, true),
                listed("/c/sub", AccessState::Denied(DenialReason::Posix), false),
            ],
            vec![
                listed("/c", AccessState::Readable, false),
                listed("/c/sub", AccessState::Readable, true),
            ],
        ] {
            assert!(!observed(&scanned[0], &scanned, &listings).complete);
        }
    }

    #[test]
    fn a_file_is_complete_when_its_metadata_was_read() {
        let scanned = tree();
        let file = observed(&scanned[1], &scanned, &[]);
        assert!(file.complete);
        assert_eq!(
            file.reclaim,
            ReclaimEstimate::for_selection([&scanned[1].size])
        );
        let mut unread = scanned[1].clone();
        unread.access = AccessState::NotScanned;
        assert!(!observed(&unread, &scanned, &[]).complete);
    }
}
