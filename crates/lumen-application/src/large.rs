//! Large folders that nothing recognises, as review candidates (ADR-0051).
//!
//! Most of a disk is never assessed when only folders some evidence names
//! become candidates. After a scan, a directory becomes a candidate of its
//! own when all of these hold:
//!
//! - removing it would free at least [`LARGE_MIN_BYTES`] now, by the measure
//!   the policy and the interface use for every candidate
//!   (`ReclaimEstimate::reclaimable_now`): clones, cloud placeholders and
//!   dataless files count for nothing, and a hard-linked file counts for
//!   nothing here (a lower bound, so a folder is never chosen for space it
//!   might not free);
//! - its size is spread out: no single subdirectory holds
//!   [`DOMINANT_PERCENT`] or more of it, so the folder is the level where the
//!   space lives (when one subdirectory dominates, that one is considered
//!   instead; files cannot be split further, so a dominant file does not
//!   move the level);
//! - it is not inside a package: a directory that people see and move as one
//!   item, such as an app or a Photos library, named by its extension
//!   ([`PACKAGES`]). A package is the level itself, however its size is
//!   spread, so a part of one (an app's `Contents`, a library's `originals`)
//!   is never offered on its own;
//! - nothing identifies it, nothing inside it and nothing around it: every
//!   identified entry is a candidate or inside one, so existing candidates win
//!   and no space is counted twice;
//! - it and every directory below it were listed in full (unread is never
//!   empty, ADR-0008), it is a directory (never a link or reparse point,
//!   which the scanner does not follow), and it and everything below it are on
//!   its parent's volume (never a mount point);
//! - it holds none of Lumen's own files (its index, its quarantine store)
//!   and is not inside them ([`LargeFolders::keeping`]);
//! - it lies strictly below a scan root, and no folder around it qualifies:
//!   the outermost one is chosen.
//!
//! At most [`MAX_LARGE`] are chosen per scan, the largest first. Only stored
//! metadata is read; no file is opened. Such a folder carries
//! [`Fact::LargeUnrecognised`], which says nothing about what it is, so the
//! policy's own rules decide it: in practice `default.review`, or `KEEP` for
//! a protected location, anything in use, a cloud item or an incomplete scan.
//! No classification rule fires, so it is never proposed for quarantine.
//!
//! [`LargeFolders`] reads the scan once, a page at a time, in the store's
//! order (flavor, directory path, name). In that order every directory's
//! subtree is contiguous and follows the directory, so a stack of the open
//! directories is all it keeps: memory follows the depth of the tree and the
//! subdirectories of the open directories, not the size of the scan.

use std::collections::{HashMap, HashSet};

use lumen_domain::{
    Basis, ByteCount, EntryKind, Evidence, Fact, FileIdentity, FilesystemEntry, PathFlavor,
    Provenance, RawPath, ReclaimEstimate, SourceName, Subject, Timestamp,
};

use crate::assess::identifies;
use crate::observe::below;

/// The least space a large folder frees: 1 GiB.
pub const LARGE_MIN_BYTES: ByteCount = ByteCount::new(1 << 30);

/// A subdirectory holding this share of a folder, in percent, or more makes
/// that subdirectory the level to look at instead.
pub const DOMINANT_PERCENT: u64 = 80;

/// The most large folders chosen per scan.
pub const MAX_LARGE: usize = 30;

/// Extensions of directories that are one item to people (macOS bundles and
/// document packages), compared without case.
pub const PACKAGES: &[&str] = &[
    "app",
    "appex",
    "band",
    "bundle",
    "fcpbundle",
    "framework",
    "imovielibrary",
    "kext",
    "logicx",
    "lrdata",
    "lrlibrary",
    "musiclibrary",
    "photoslibrary",
    "plugin",
    "pvm",
    "sparsebundle",
    "tvlibrary",
    "utm",
    "vmwarevm",
    "xcarchive",
    "xpc",
];

/// Whether a directory called `name` is a package.
fn is_package(name: &[u8]) -> bool {
    name.iter().rposition(|&b| b == b'.').is_some_and(|dot| {
        let extension = name.get(dot + 1..).unwrap_or_default();
        PACKAGES
            .iter()
            .any(|package| package.as_bytes().eq_ignore_ascii_case(extension))
    })
}

/// The source named in the provenance of [`Fact::LargeUnrecognised`].
pub const LARGE_SOURCE: &str = "lumen.large-folder";

/// One chosen folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LargeFolder {
    /// The folder as scanned.
    pub entry: FilesystemEntry,
    /// The space removing it would free now, as measured for the choice.
    pub reclaimable: ByteCount,
}

impl LargeFolder {
    /// The evidence that makes it a candidate, dated by the scan
    /// (`scanned_at`) so assessing the same scan again records nothing new.
    pub fn evidence(&self, scanned_at: Timestamp) -> Option<Evidence> {
        Some(Evidence {
            subject: Subject::File {
                identity: self.entry.identity().clone(),
            },
            fact: Fact::LargeUnrecognised {
                reclaimable: self.reclaimable,
            },
            basis: Basis::Observed,
            provenance: Provenance {
                source: SourceName::new(LARGE_SOURCE).ok()?,
                observed_at: scanned_at,
            },
        })
    }
}

/// One open directory: one stored directory path, while its subtree is read.
#[derive(Debug)]
#[allow(clippy::struct_excessive_bools)] // Independent conditions, not a state machine.
struct Frame {
    flavor: PathFlavor,
    /// The stored directory path: the directory's path and a separator.
    key: Vec<u8>,
    /// The directory's own entry, from its parent's listing, when stored.
    entry: Option<FilesystemEntry>,
    /// Evidence identifies the directory itself.
    named: bool,
    /// Evidence identifies a directory around it.
    within_named: bool,
    /// It is a package.
    package: bool,
    /// A directory around it is a package.
    within_package: bool,
    /// Evidence identifies something below it.
    holds_named: bool,
    /// Its subdirectories seen so far, by name, until their own frames open.
    pending: HashMap<Vec<u8>, (FilesystemEntry, bool)>,
    /// What removing everything below it frees now.
    bytes: u64,
    /// The most any one subdirectory frees.
    largest_child: u64,
    /// It and everything below it were listed in full.
    complete: bool,
    /// Something below it is on another volume.
    mixed: bool,
}

impl Frame {
    fn new(flavor: PathFlavor, key: Vec<u8>) -> Self {
        Self {
            flavor,
            key,
            entry: None,
            named: false,
            within_named: false,
            package: false,
            within_package: false,
            holds_named: false,
            pending: HashMap::new(),
            bytes: 0,
            largest_child: 0,
            complete: true,
            mixed: false,
        }
    }

    fn spread(&self) -> bool {
        u128::from(self.largest_child) * 100 < u128::from(self.bytes) * u128::from(DOMINANT_PERCENT)
    }
}

const fn separator(flavor: PathFlavor) -> u8 {
    match flavor {
        PathFlavor::Unix => b'/',
        PathFlavor::Windows => b'\\',
    }
}

/// Finds the large folders of one scan, fed its entries in the store's order.
#[derive(Debug)]
pub struct LargeFolders {
    roots: Vec<RawPath>,
    unlisted: HashSet<RawPath>,
    own: Vec<RawPath>,
    stack: Vec<Frame>,
    found: Vec<LargeFolder>,
}

impl LargeFolders {
    /// Starts reading a scan of `roots` whose `unlisted` directories were not
    /// listed in full.
    pub fn new(roots: &[RawPath], unlisted: impl IntoIterator<Item = RawPath>) -> Self {
        Self {
            roots: roots.to_vec(),
            unlisted: unlisted.into_iter().collect(),
            own: Vec::new(),
            stack: Vec::new(),
            found: Vec::new(),
        }
    }

    /// Never chooses a folder that holds or lies inside one of `own`:
    /// Lumen's own files and folders, such as its index and its quarantine
    /// store.
    #[must_use]
    pub fn keeping(mut self, own: impl IntoIterator<Item = RawPath>) -> Self {
        self.own.extend(own);
        self
    }

    /// Reads the next `page` of the scan, with the `evidence` the producers
    /// stated about it.
    pub fn observe<'a>(
        &mut self,
        page: &[FilesystemEntry],
        evidence: impl IntoIterator<Item = &'a Evidence>,
    ) {
        let named: HashSet<&FileIdentity> = evidence
            .into_iter()
            .filter(|record| identifies(&record.fact))
            .filter_map(|record| match &record.subject {
                Subject::File { identity } => Some(identity),
                _ => None,
            })
            .collect();
        for entry in page {
            self.add(entry, named.contains(entry.identity()));
        }
    }

    fn add(&mut self, entry: &FilesystemEntry, named: bool) {
        let flavor = entry.path.flavor();
        let bytes = entry.path.as_bytes();
        let cut = bytes
            .iter()
            .rposition(|&b| b == separator(flavor))
            .map_or(0, |at| at + 1);
        let (key, name) = bytes.split_at(cut);
        self.enter(flavor, key);
        let unlisted = entry.kind == EntryKind::Directory && self.unlisted.contains(&entry.path);
        let Some(frame) = self.stack.last_mut() else {
            return;
        };
        if name.is_empty() {
            // The directory of its own group, such as the root `/`.
            if frame.entry.is_none() {
                frame.entry = Some(entry.clone());
                frame.named |= named;
                frame.complete &= !unlisted;
            }
            return;
        }
        frame.holds_named |= named;
        frame.mixed |= frame
            .entry
            .as_ref()
            .is_some_and(|own| own.identity().volume != entry.identity().volume);
        if entry.kind == EntryKind::Directory {
            frame.complete &= !unlisted;
            frame.pending.insert(name.to_vec(), (entry.clone(), named));
        } else {
            let freed = ReclaimEstimate::for_selection([&entry.size]).reclaimable_now;
            frame.bytes = frame.bytes.saturating_add(freed.get());
        }
    }

    /// Makes the frame of the stored directory path `key` the top one,
    /// closing every frame whose subtree it is past.
    fn enter(&mut self, flavor: PathFlavor, key: &[u8]) {
        while self
            .stack
            .last()
            .is_some_and(|top| top.flavor != flavor || !key.starts_with(&top.key))
        {
            self.close();
        }
        loop {
            let Some(top) = self.stack.last_mut() else {
                self.stack.push(Frame::new(flavor, key.to_vec()));
                return;
            };
            if top.key == key {
                return;
            }
            let rest = key.get(top.key.len()..).unwrap_or_default();
            let Some(at) = rest.iter().position(|&b| b == separator(flavor)) else {
                self.stack.push(Frame::new(flavor, key.to_vec()));
                return;
            };
            let name = rest.get(..at).unwrap_or_default();
            let mut frame = Frame::new(
                flavor,
                key.get(..top.key.len() + at + 1)
                    .unwrap_or_default()
                    .to_vec(),
            );
            frame.within_named = top.within_named || top.named;
            frame.within_package = top.within_package || top.package;
            frame.package = is_package(name);
            if let Some((entry, named)) = top.pending.remove(name) {
                frame.complete = !self.unlisted.contains(&entry.path);
                frame.named = named;
                frame.entry = Some(entry);
            }
            self.stack.push(frame);
        }
    }

    /// Closes the top frame: decides whether it is a large folder and adds
    /// it to its parent.
    fn close(&mut self) {
        let Some(frame) = self.stack.pop() else {
            return;
        };
        let own_volume = frame.entry.as_ref().map(|own| &own.identity().volume);
        let parent_volume = self
            .stack
            .last()
            .and_then(|parent| parent.entry.as_ref())
            .map(|own| &own.identity().volume);
        let same_volume = parent_volume.is_some() && parent_volume == own_volume;
        let other_volume = parent_volume.is_some() && !same_volume;
        if let Some(parent) = self.stack.last_mut() {
            parent.bytes = parent.bytes.saturating_add(frame.bytes);
            parent.largest_child = parent.largest_child.max(frame.bytes);
            parent.complete &= frame.complete;
            parent.holds_named |= frame.holds_named || frame.named;
            parent.mixed |= frame.mixed || other_volume;
        }
        let Some(entry) = frame.entry.as_ref() else {
            return;
        };
        let qualifies = entry.kind == EntryKind::Directory
            && self.roots.iter().any(|root| below(&entry.path, root))
            && !(frame.named || frame.within_named || frame.holds_named)
            && !frame.within_package
            && !self.own.iter().any(|own| {
                own == &entry.path || below(own, &entry.path) || below(&entry.path, own)
            })
            && frame.complete
            && !frame.mixed
            && same_volume
            && frame.bytes >= LARGE_MIN_BYTES.get()
            && (frame.package || frame.spread());
        if qualifies {
            self.found.push(LargeFolder {
                entry: entry.clone(),
                reclaimable: ByteCount::new(frame.bytes),
            });
        }
    }

    /// The large folders: the outermost qualifying ones, the largest first
    /// (then by path), at most [`MAX_LARGE`].
    pub fn finish(mut self) -> Vec<LargeFolder> {
        while !self.stack.is_empty() {
            self.close();
        }
        let mut found = self.found;
        found.sort_by(|a, b| a.entry.path.cmp(&b.entry.path));
        let mut outermost: Vec<LargeFolder> = Vec::new();
        for folder in found {
            if !outermost
                .iter()
                .any(|outer| below(&folder.entry.path, &outer.entry.path))
            {
                outermost.push(folder);
            }
        }
        outermost.sort_by(|a, b| {
            b.reclaimable
                .cmp(&a.reclaimable)
                .then_with(|| a.entry.path.cmp(&b.entry.path))
        });
        outermost.truncate(MAX_LARGE);
        outermost
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;

    use lumen_domain::{
        AccessState, AppDataKind, EntryTimes, FileId, Protection, SizeFacts, SizeFlags, VolumeId,
    };

    use super::*;

    const GIB: u64 = 1 << 30;

    fn raw(path: &str) -> RawPath {
        RawPath::from_unix_bytes(path.as_bytes().to_vec()).unwrap_or_else(|_| unreachable!())
    }

    fn at(path: &str, kind: EntryKind, bytes: u64, volume: &str) -> FilesystemEntry {
        let file = u128::from(
            path.bytes()
                .fold(7_u64, |h, b| h.wrapping_mul(31).wrapping_add(u64::from(b))),
        );
        FilesystemEntry {
            path: raw(path),
            kind,
            size: SizeFacts {
                identity: FileIdentity {
                    volume: VolumeId::new(volume).unwrap_or_else(|_| unreachable!()),
                    file: FileId::new(file),
                },
                logical: ByteCount::new(bytes),
                allocated: ByteCount::new(bytes),
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

    fn dir(path: &str) -> FilesystemEntry {
        at(path, EntryKind::Directory, 0, "v")
    }

    fn file(path: &str, bytes: u64) -> FilesystemEntry {
        at(path, EntryKind::File, bytes, "v")
    }

    /// The store's order: flavor, directory path (up to the last
    /// separator), name.
    fn stored(mut entries: Vec<FilesystemEntry>) -> Vec<FilesystemEntry> {
        entries.sort_by(|a, b| {
            let split = |e: &FilesystemEntry| {
                let bytes = e.path.as_bytes();
                let cut = bytes.iter().rposition(|&b| b == b'/').map_or(0, |i| i + 1);
                (bytes[..cut].to_vec(), bytes[cut..].to_vec())
            };
            split(a).cmp(&split(b))
        });
        entries
    }

    fn named(entry: &FilesystemEntry) -> Evidence {
        Evidence {
            subject: Subject::File {
                identity: entry.identity().clone(),
            },
            fact: Fact::AppDataLocation {
                kind: AppDataKind::Caches,
            },
            basis: Basis::Observed,
            provenance: Provenance {
                source: SourceName::new("test").unwrap_or_else(|_| unreachable!()),
                observed_at: Timestamp::UNIX_EPOCH,
            },
        }
    }

    /// The chosen paths, reading the entries in pages of `page`.
    fn chosen(
        entries: Vec<FilesystemEntry>,
        evidence: &[Evidence],
        unlisted: &[&str],
        page: usize,
    ) -> Vec<(String, u64)> {
        let mut large = LargeFolders::new(&[raw("/h")], unlisted.iter().map(|p| raw(p)));
        for chunk in stored(entries).chunks(page) {
            large.observe(chunk, evidence);
        }
        large
            .finish()
            .into_iter()
            .map(|f| {
                (
                    String::from_utf8_lossy(f.entry.path.as_bytes()).into_owned(),
                    f.reclaimable.get() / GIB,
                )
            })
            .collect()
    }

    /// `/h/name` holding `files` of `each` GiB, side by side.
    fn spread(name: &str, files: u64, each: u64) -> Vec<FilesystemEntry> {
        let mut entries = vec![dir(&format!("/h/{name}"))];
        entries.extend((0..files).map(|n| file(&format!("/h/{name}/f{n}"), each * GIB)));
        entries
    }

    fn home(more: Vec<FilesystemEntry>) -> Vec<FilesystemEntry> {
        let mut entries = vec![dir("/h")];
        entries.extend(more);
        entries
    }

    #[test]
    fn a_spread_folder_of_at_least_one_gib_is_chosen_and_a_smaller_one_is_not() {
        let mut entries = home(spread("big", 2, 1));
        entries.push(dir("/h/small"));
        entries.push(file("/h/small/a", GIB - 1));
        assert_eq!(chosen(entries, &[], &[], 2), [("/h/big".to_owned(), 2)]);
    }

    #[test]
    fn a_dominant_subdirectory_is_the_level_but_a_dominant_file_is_not() {
        let entries = home(vec![
            dir("/h/vm"),
            file("/h/vm/notes", GIB / 4),
            dir("/h/vm/disk"),
            file("/h/vm/disk/a", 2 * GIB),
            file("/h/vm/disk/b", 2 * GIB),
            dir("/h/one"),
            file("/h/one/image", 5 * GIB),
            file("/h/one/log", GIB / 8),
        ]);
        assert_eq!(
            chosen(entries, &[], &[], 3),
            [("/h/one".to_owned(), 5), ("/h/vm/disk".to_owned(), 4)],
            "/h/vm/disk holds 94 % of /h/vm; a single file cannot be split"
        );
    }

    #[test]
    fn a_package_is_one_item_however_its_size_is_spread() {
        let entries = home(vec![
            dir("/h/Lib.photoslibrary"),
            dir("/h/Lib.photoslibrary/originals"),
            dir("/h/Lib.photoslibrary/originals/0"),
            file("/h/Lib.photoslibrary/originals/0/a", 2 * GIB),
            dir("/h/Lib.photoslibrary/originals/1"),
            file("/h/Lib.photoslibrary/originals/1/a", 2 * GIB),
            dir("/h/u"),
            dir("/h/u/Big.APP"),
            dir("/h/u/Big.APP/Contents"),
            file("/h/u/Big.APP/Contents/a", GIB),
            file("/h/u/Big.APP/Contents/b", GIB),
        ]);
        assert_eq!(
            chosen(entries, &[], &[], 2),
            [
                ("/h/Lib.photoslibrary".to_owned(), 4),
                ("/h/u/Big.APP".to_owned(), 2)
            ],
            "never `originals` or `Contents`; a dominant package is the level"
        );
        assert!(!is_package(b"photoslibrary") && !is_package(b"v2.0") && is_package(b"x.app"));
    }

    #[test]
    fn lumens_own_files_and_folders_are_never_chosen() {
        let mut entries = home(spread("data", 2, 1));
        entries.push(file("/h/data/index.db", GIB / 2));
        entries.extend(spread("store", 2, 1));
        entries.push(dir("/h/store/q"));
        entries.extend((0..2).map(|n| file(&format!("/h/store/q/f{n}"), 2 * GIB)));
        entries.extend(spread("beside", 2, 1));
        let mut large = LargeFolders::new(&[raw("/h")], [])
            .keeping([raw("/h/data/index.db"), raw("/h/store/q")]);
        large.observe(&stored(entries), []);
        let paths: Vec<RawPath> = large.finish().into_iter().map(|f| f.entry.path).collect();
        assert_eq!(paths, [raw("/h/beside")]);
    }

    #[test]
    fn the_outermost_spread_folder_wins_and_the_root_is_never_chosen() {
        let mut entries = home(vec![dir("/h/p")]);
        for project in ["a", "b"] {
            entries.push(dir(&format!("/h/p/{project}")));
            for n in 0..2 {
                entries.push(file(&format!("/h/p/{project}/f{n}"), GIB));
            }
        }
        assert_eq!(chosen(entries, &[], &[], 1), [("/h/p".to_owned(), 4)]);
    }

    #[test]
    fn at_most_thirty_are_chosen_the_largest_first() {
        let mut entries = vec![dir("/h")];
        for n in 1..=35 {
            entries.extend(spread(&format!("d{n:02}"), 2, n));
        }
        let chosen = chosen(entries, &[], &[], 50);
        assert_eq!(chosen.len(), MAX_LARGE);
        assert_eq!(chosen[0], ("/h/d35".to_owned(), 70));
        assert_eq!(chosen[29], ("/h/d06".to_owned(), 12));
    }

    #[test]
    fn nothing_inside_around_or_equal_to_an_identified_entry_is_chosen() {
        let mut entries = home(vec![dir("/h/lib"), dir("/h/lib/cache")]);
        entries.extend([
            file("/h/lib/cache/a", 3 * GIB),
            dir("/h/lib/docs"),
            file("/h/lib/docs/a", GIB),
            file("/h/lib/docs/b", GIB),
        ]);
        entries.extend(spread("named", 2, 1));
        entries.extend([dir("/h/named/inner")]);
        entries.extend((0..2).map(|n| file(&format!("/h/named/inner/f{n}"), 2 * GIB)));
        let evidence = [named(&dir("/h/lib/cache")), named(&dir("/h/named"))];
        assert_eq!(
            chosen(entries, &evidence, &[], 2),
            [("/h/lib/docs".to_owned(), 2)],
            "/h/lib holds a candidate; /h/named is one; /h/named/inner is inside one"
        );
    }

    #[test]
    fn a_folder_not_listed_in_full_is_never_chosen_but_its_complete_parts_may_be() {
        let mut entries = home(spread("partial", 2, 1));
        entries.extend([dir("/h/partial/locked")]);
        entries.extend(spread("denied", 3, 1));
        entries.extend([dir("/h/outer"), dir("/h/outer/x"), dir("/h/outer/y")]);
        entries.extend((0..2).map(|n| file(&format!("/h/outer/x/f{n}"), GIB)));
        entries.extend((0..2).map(|n| file(&format!("/h/outer/y/f{n}"), GIB)));
        entries.push(dir("/h/outer/z"));
        assert_eq!(
            chosen(
                entries,
                &[],
                &["/h/partial/locked", "/h/denied", "/h/outer/z"],
                4
            ),
            [("/h/outer/x".to_owned(), 2), ("/h/outer/y".to_owned(), 2)]
        );
    }

    #[test]
    fn links_mount_points_and_other_volumes_are_never_chosen() {
        let mut entries = home(vec![
            at("/h/link", EntryKind::Symlink, 0, "v"),
            at("/h/mnt", EntryKind::Directory, 0, "other"),
        ]);
        entries.extend((0..2).map(|n| at(&format!("/h/mnt/f{n}"), EntryKind::File, GIB, "other")));
        entries.extend(spread("mixed", 2, 1));
        entries.push(at("/h/mixed/vol", EntryKind::Directory, 0, "other"));
        assert_eq!(chosen(entries, &[], &[], 3), []);
    }

    #[test]
    fn sibling_names_that_sort_between_a_folder_and_its_contents_are_kept_apart() {
        // `/h/a-b/` sorts before `/h/a/`, between `/h/a`'s entry and its contents.
        let mut entries = home(spread("a", 2, 1));
        entries.extend(spread("a-b", 3, 1));
        entries.extend([dir("/h/a/c"), file("/h/a/c/f", GIB / 2)]);
        assert_eq!(
            chosen(entries, &[], &[], 1),
            [("/h/a-b".to_owned(), 3), ("/h/a".to_owned(), 2)]
        );
    }

    #[test]
    fn the_evidence_says_nothing_about_what_the_folder_is() {
        let folder = LargeFolder {
            entry: dir("/h/big"),
            reclaimable: ByteCount::new(2 * GIB),
        };
        let evidence = folder.evidence(Timestamp::UNIX_EPOCH);
        assert_eq!(
            evidence.as_ref().map(|e| &e.fact),
            Some(&Fact::LargeUnrecognised {
                reclaimable: ByteCount::new(2 * GIB)
            })
        );
        assert!(evidence.is_some_and(|e| identifies(&e.fact)));
    }
}
