//! Cloud sync domains as the scan saw them (security audit FS-2).
//!
//! Moving an item out of a cloud provider's sync root makes the provider
//! delete it in the cloud and on every other device, so an item that is
//! itself materialised and local is still cloud-backed when it lies at or
//! below a cloud-backed directory, or when it contains a cloud-backed entry:
//!
//! - **ancestors:** a directory the platform flags as a placeholder, dataless
//!   or a sync root (macOS `EF_IS_SYNC_ROOT`; Windows cloud reparse tags and
//!   pinned/unpinned attributes) puts everything below it in its domain, such
//!   as a materialised `node_modules` in a Dropbox project;
//! - **descendants:** a directory with a placeholder or dataless entry
//!   anywhere below it holds cloud content, so moving it would take that
//!   content along.
//!
//! Only the scan's own flags are used: nothing is opened, hydrated or read.
//! The scan is read in pages, so [`CloudDomains::add`] runs over every page
//! first; [`CloudDomains::contains`] then answers for any entry. Paths are
//! compared as the scan stored them, which is consistent within one scan.
//! Pure and platform-independent.

use std::collections::BTreeSet;
use std::ops::Bound;

use lumen_domain::{EntryKind, FilesystemEntry, PathFlavor, RawPath};

/// The cloud-backed directories of one scan and the directories holding
/// cloud-backed entries.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct CloudDomains {
    /// Flagged directories, each with a trailing separator.
    flagged: BTreeSet<Vec<u8>>,
    /// The parent of every flagged entry, each with a trailing separator.
    holders: BTreeSet<Vec<u8>>,
}

pub(crate) fn separator(path: &RawPath) -> u8 {
    match path.flavor() {
        PathFlavor::Unix => b'/',
        PathFlavor::Windows => b'\\',
    }
}

/// `path` with a trailing separator, so a prefix test matches whole names.
pub(crate) fn key(path: &RawPath) -> Vec<u8> {
    let sep = separator(path);
    let mut bytes = path.as_bytes().to_vec();
    if bytes.last() != Some(&sep) {
        bytes.push(sep);
    }
    bytes
}

/// Whether the entry's own flags say it belongs to a cloud provider. A file
/// whose metadata was not read has no flags to go by.
fn flagged(entry: &FilesystemEntry) -> bool {
    let read = entry.kind == EntryKind::Directory || entry.access.is_observed();
    read && (entry.size.flags.cloud_placeholder || entry.size.flags.dataless)
}

impl CloudDomains {
    /// No domains.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Pass 1: notes the flagged entries of one page of the scan.
    pub fn add(&mut self, page: &[FilesystemEntry]) {
        for entry in page.iter().filter(|e| flagged(e)) {
            let own = key(&entry.path);
            let sep = separator(&entry.path);
            // The name ends before the trailing separator; the parent keeps
            // its own separator.
            if let Some(end) = own[..own.len() - 1].iter().rposition(|b| *b == sep) {
                self.holders.insert(own[..=end].to_vec());
            }
            if entry.kind == EntryKind::Directory {
                self.flagged.insert(own);
            }
        }
    }

    /// Whether `entry` is cloud-backed: flagged itself, at or below a flagged
    /// directory, or a directory with a flagged entry below it.
    #[must_use]
    pub fn contains(&self, entry: &FilesystemEntry) -> bool {
        if flagged(entry) {
            return true;
        }
        let own = key(&entry.path);
        let sep = separator(&entry.path);
        let below_flagged = own
            .iter()
            .enumerate()
            .filter(|(_, b)| **b == sep)
            .any(|(end, _)| self.flagged.contains(&own[..=end]));
        // Keys with `own` as a prefix sort together, right from `own`.
        let holds_flagged = entry.kind == EntryKind::Directory
            && self
                .holders
                .range::<[u8], _>((Bound::Included(own.as_slice()), Bound::Unbounded))
                .next()
                .is_some_and(|holder| holder.starts_with(&own));
        below_flagged || holds_flagged
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;

    use lumen_domain::{
        AccessState, ByteCount, EntryTimes, FileId, FileIdentity, Protection, SizeFacts, SizeFlags,
        VolumeId,
    };

    use super::*;

    fn entry(path: RawPath, kind: EntryKind, cloud: bool, dataless: bool) -> FilesystemEntry {
        FilesystemEntry {
            path,
            kind,
            size: SizeFacts {
                identity: FileIdentity {
                    volume: VolumeId::new("v").unwrap_or_else(|_| unreachable!()),
                    file: FileId::new(1),
                },
                logical: ByteCount::ZERO,
                allocated: ByteCount::ZERO,
                private: None,
                clone_id: None,
                link_count: NonZeroU32::MIN,
                flags: SizeFlags {
                    cloud_placeholder: cloud,
                    dataless,
                    ..SizeFlags::default()
                },
            },
            times: EntryTimes::default(),
            protection: Protection::default(),
            access: AccessState::Readable,
        }
    }

    fn unix(path: &str) -> RawPath {
        RawPath::from_unix_bytes(path.as_bytes().to_vec()).unwrap_or_else(|_| unreachable!())
    }

    fn dir(path: &str) -> FilesystemEntry {
        entry(unix(path), EntryKind::Directory, false, false)
    }

    fn file(path: &str) -> FilesystemEntry {
        entry(unix(path), EntryKind::File, false, false)
    }

    fn domains(pages: &[&[FilesystemEntry]]) -> CloudDomains {
        let mut domains = CloudDomains::new();
        for page in pages {
            domains.add(page);
        }
        domains
    }

    #[test]
    fn everything_below_a_sync_root_is_cloud_backed() {
        let root = "/Users/ana/Library/CloudStorage/Dropbox";
        let domains = domains(&[&[entry(unix(root), EntryKind::Directory, true, false)]]);
        for path in [
            "/Users/ana/Library/CloudStorage/Dropbox/project/node_modules",
            "/Users/ana/Library/CloudStorage/Dropbox/a/b/c/d",
        ] {
            assert!(domains.contains(&dir(path)), "{path}");
            assert!(domains.contains(&file(path)), "{path}");
        }
        for path in [
            "/Users/ana/Library/CloudStorage/Dropbox2/node_modules",
            "/Users/ana/Library/CloudStorage/Drop",
            "/Users/ana/project/node_modules",
        ] {
            assert!(!domains.contains(&dir(path)), "{path}");
        }
    }

    #[test]
    fn a_directory_holding_a_placeholder_is_cloud_backed() {
        let domains = domains(&[
            &[file("/Users/ana/work/a")],
            &[entry(
                unix("/Users/ana/work/docs/sub/report.pdf"),
                EntryKind::File,
                false,
                true,
            )],
        ]);
        for path in [
            "/Users/ana/work",
            "/Users/ana/work/docs",
            "/Users/ana/work/docs/sub",
            "/Users/ana",
            "/",
        ] {
            assert!(domains.contains(&dir(path)), "{path}");
        }
        for path in [
            "/Users/ana/work/doc",
            "/Users/ana/work/docs/other",
            "/Users/ana/wor",
        ] {
            assert!(!domains.contains(&dir(path)), "{path}");
        }
        // A file has no descendants; a sibling of a placeholder is local.
        assert!(!domains.contains(&file("/Users/ana/work/docs/sub/notes.txt")));
    }

    #[test]
    fn a_dataless_directory_marks_its_ancestors_and_descendants() {
        let domains = domains(&[&[entry(
            unix("/Users/ana/Library/Mobile Documents/com~apple~CloudDocs/Photos"),
            EntryKind::Directory,
            false,
            true,
        )]]);
        assert!(domains.contains(&dir("/Users/ana/Library/Mobile Documents")));
        assert!(domains.contains(&file(
            "/Users/ana/Library/Mobile Documents/com~apple~CloudDocs/Photos/x"
        )));
        assert!(!domains.contains(&dir("/Users/ana/Library/Caches")));
    }

    #[test]
    fn unread_file_metadata_flags_nothing() {
        let mut unread = entry(unix("/Users/ana/x/f"), EntryKind::File, true, true);
        unread.access = AccessState::NotScanned;
        let domains = domains(&[&[unread.clone()]]);
        assert!(!domains.contains(&dir("/Users/ana/x")));
        assert!(!domains.contains(&unread));
    }

    #[test]
    fn windows_paths_use_backslashes() {
        let windows = |p: &str| {
            let wide: Vec<u16> = p.encode_utf16().collect();
            RawPath::from_windows_wide(&wide).unwrap_or_else(|_| unreachable!())
        };
        let domains = domains(&[&[entry(
            windows(r"C:\Users\ana\OneDrive"),
            EntryKind::Directory,
            true,
            false,
        )]]);
        let below = entry(
            windows(r"C:\Users\ana\OneDrive\app\node_modules"),
            EntryKind::Directory,
            false,
            false,
        );
        let beside = entry(
            windows(r"C:\Users\ana\OneDriveTemp\x"),
            EntryKind::Directory,
            false,
            false,
        );
        let parent = entry(windows(r"C:\Users\ana"), EntryKind::Directory, false, false);
        assert!(domains.contains(&below));
        assert!(!domains.contains(&beside));
        assert!(domains.contains(&parent), "holds the sync root");
    }

    #[test]
    fn a_volume_root_key_keeps_one_separator() {
        assert_eq!(key(&unix("/")), b"/".to_vec());
        assert_eq!(key(&unix("/a")), b"/a/".to_vec());
    }
}
