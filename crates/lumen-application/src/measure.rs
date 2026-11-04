//! Exact sizes for the files below a candidate (ADR-0017, 2026-11-04
//! amendment).
//!
//! A listing can leave a file's reclaim unknown: a Windows directory record has
//! no hard-link count, so every listed file may share blocks and frees nothing
//! for certain. Before a candidate is estimated, [`measure`] re-reads such files
//! through the [`CandidateVerifier`] and keeps the fresh size facts where the
//! object is unchanged. Everything else keeps the listing's figures, which are a
//! lower bound: the estimate never over-promises.

use lumen_domain::{EntryKind, FilesystemEntry};

use crate::ports::CandidateVerifier;

/// Files re-read per candidate at most; the rest keep the listing's lower
/// bound.
pub const LIMIT: usize = 100_000;

/// What [`measure`] did.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Measured {
    /// Files whose size facts were replaced by verified ones.
    pub verified: usize,
    /// Files the verifier could not confirm (gone, refused or replaced); they
    /// keep the listing's figures.
    pub unconfirmed: usize,
    /// Files left unread because `limit` was reached.
    pub skipped: usize,
}

/// Whether the listing left `entry`'s reclaim unknown and a re-read can help.
/// Cloud placeholders and dataless files are never opened: they free nothing
/// whatever their links (and opening one must never hydrate it).
fn needs_measuring(entry: &FilesystemEntry) -> bool {
    let flags = entry.size.flags;
    entry.kind == EntryKind::File
        && entry.size.private.is_none()
        && flags.may_share_blocks
        && !flags.cloud_placeholder
        && !flags.dataless
}

/// Re-reads, through `verifier`, up to `limit` files in `entries` whose
/// reclaim the listing left unknown, and replaces their size facts with the
/// verified ones when the object is still the same identity and kind. Paths,
/// times, protection and access states are never changed.
pub fn measure(
    entries: &mut [FilesystemEntry],
    verifier: &dyn CandidateVerifier,
    limit: usize,
) -> Measured {
    let mut measured = Measured::default();
    for entry in entries.iter_mut().filter(|entry| needs_measuring(entry)) {
        if measured.verified + measured.unconfirmed >= limit {
            measured.skipped += 1;
            continue;
        }
        match verifier.verify(&entry.path, entry.identity()) {
            Ok(fresh) if fresh.kind == entry.kind && fresh.identity() == entry.identity() => {
                entry.size = fresh.size;
                measured.verified += 1;
            }
            Ok(_) | Err(_) => measured.unconfirmed += 1,
        }
    }
    measured
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;
    use std::sync::Mutex;

    use lumen_domain::{
        AccessState, ByteCount, EntryTimes, FileId, FileIdentity, Protection, RawPath,
        ReclaimEstimate, SizeFacts, SizeFlags, VolumeId,
    };

    use super::*;
    use crate::ports::EnumerateError;

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
                flags: SizeFlags {
                    may_share_blocks: kind == EntryKind::File,
                    ..SizeFlags::default()
                },
            },
            times: EntryTimes::default(),
            protection: Protection::default(),
            access: AccessState::Readable,
        }
    }

    /// Answers like the Windows verifier on NTFS: exact, with `links` per
    /// file ID; IDs in `gone` are unobservable, IDs in `replaced` are another
    /// object now. Records every path it was asked about.
    struct Verifier {
        links: fn(u128) -> u32,
        gone: Vec<u128>,
        replaced: Vec<u128>,
        asked: Mutex<Vec<RawPath>>,
    }

    impl Verifier {
        fn new(links: fn(u128) -> u32) -> Self {
            Self {
                links,
                gone: Vec::new(),
                replaced: Vec::new(),
                asked: Mutex::new(Vec::new()),
            }
        }

        fn asked(&self) -> Vec<RawPath> {
            self.asked.lock().map(|a| a.clone()).unwrap_or_default()
        }
    }

    impl CandidateVerifier for Verifier {
        fn verify(
            &self,
            path: &RawPath,
            expected: &FileIdentity,
        ) -> Result<FilesystemEntry, EnumerateError> {
            if let Ok(mut asked) = self.asked.lock() {
                asked.push(path.clone());
            }
            let id = expected.file.get();
            if self.gone.contains(&id) {
                return Err(EnumerateError::Unobservable {
                    path: path.clone(),
                    access: AccessState::NotScanned,
                });
            }
            let path_text = String::from_utf8_lossy(path.as_bytes()).into_owned();
            let mut fresh = entry(&path_text, EntryKind::File, id, 4096);
            if self.replaced.contains(&id) {
                fresh.size.identity.file = FileId::new(id + 1000);
            }
            fresh.size.link_count = NonZeroU32::new((self.links)(id)).unwrap_or(NonZeroU32::MIN);
            fresh.size.flags.may_share_blocks = false;
            Ok(fresh)
        }
    }

    fn subtree() -> Vec<FilesystemEntry> {
        vec![
            entry("/c", EntryKind::Directory, 1, 0),
            entry("/c/a", EntryKind::File, 2, 4096),
            entry("/c/b", EntryKind::File, 3, 4096),
            entry("/c/link", EntryKind::Symlink, 4, 0),
        ]
    }

    fn reclaim(entries: &[FilesystemEntry]) -> ReclaimEstimate {
        ReclaimEstimate::for_selection(entries.iter().map(|e| &e.size))
    }

    #[test]
    fn listed_files_free_nothing_for_certain_until_measured() {
        let mut entries = subtree();
        assert_eq!(reclaim(&entries).reclaimable_now, ByteCount::ZERO);
        assert!(!reclaim(&entries).exact);
        let verifier = Verifier::new(|_| 1);
        let measured = measure(&mut entries, &verifier, LIMIT);
        assert_eq!(
            measured,
            Measured {
                verified: 2,
                ..Measured::default()
            }
        );
        assert_eq!(reclaim(&entries).reclaimable_now, ByteCount::new(8192));
        assert!(reclaim(&entries).exact);
        assert_eq!(verifier.asked().len(), 2, "only files are re-read");
    }

    #[test]
    fn a_file_with_a_link_elsewhere_still_frees_nothing() {
        let mut entries = subtree();
        // `/c/b` has a second link outside the candidate (a pnpm store).
        let verifier = Verifier::new(|id| if id == 3 { 2 } else { 1 });
        measure(&mut entries, &verifier, LIMIT);
        assert_eq!(reclaim(&entries).reclaimable_now, ByteCount::new(4096));
    }

    #[test]
    fn unconfirmed_files_keep_the_listing() {
        let mut entries = subtree();
        let listed = entries.clone();
        let mut verifier = Verifier::new(|_| 1);
        verifier.gone = vec![2];
        verifier.replaced = vec![3];
        let measured = measure(&mut entries, &verifier, LIMIT);
        assert_eq!(measured.unconfirmed, 2);
        assert_eq!(measured.verified, 0);
        assert_eq!(entries, listed, "nothing changed");
        assert_eq!(reclaim(&entries).reclaimable_now, ByteCount::ZERO);
    }

    #[test]
    fn placeholders_dataless_and_measured_files_are_never_opened() {
        let mut entries = subtree();
        entries[1].size.flags.cloud_placeholder = true;
        entries[2].size.flags.dataless = true;
        let mut exact = entry("/c/exact", EntryKind::File, 5, 4096);
        exact.size.flags.may_share_blocks = false;
        let mut private = entry("/c/private", EntryKind::File, 6, 4096);
        private.size.private = Some(ByteCount::new(4096));
        entries.push(exact);
        entries.push(private);
        let verifier = Verifier::new(|_| 1);
        assert_eq!(measure(&mut entries, &verifier, LIMIT), Measured::default());
        assert_eq!(verifier.asked().len(), 0, "nothing was opened");
    }

    #[test]
    fn the_limit_leaves_the_rest_as_a_lower_bound() {
        let mut entries = subtree();
        let verifier = Verifier::new(|_| 1);
        let measured = measure(&mut entries, &verifier, 1);
        assert_eq!(
            measured,
            Measured {
                verified: 1,
                unconfirmed: 0,
                skipped: 1
            }
        );
        let estimate = reclaim(&entries);
        assert_eq!(estimate.reclaimable_now, ByteCount::new(4096));
        assert!(!estimate.exact, "still a lower bound");
    }

    #[test]
    fn only_size_facts_change() {
        let mut entries = subtree();
        entries[1].access = AccessState::Readable;
        entries[1].protection.hidden = true;
        let before = entries[1].clone();
        let verifier = Verifier::new(|_| 1);
        measure(&mut entries, &verifier, LIMIT);
        assert_eq!(entries[1].path, before.path);
        assert_eq!(entries[1].times, before.times);
        assert_eq!(entries[1].protection, before.protection);
        assert_eq!(entries[1].access, before.access);
        assert_ne!(entries[1].size, before.size);
    }
}
