//! Per-directory size totals (ADR-0017).
//!
//! Totals are computed per *file identity*, not per path, so hard links never
//! double-count:
//!
//! - **usage**: a file counts once toward every directory that contains at least
//!   one of its links (the union of the links' ancestor chains);
//! - **reclaimable**: a file is reclaimable from directory `D` only if *all* of
//!   its links are under `D`, i.e. `D` is an ancestor of the links' deepest common
//!   directory. Files with links outside the scanned roots are reclaimable nowhere.
//!
//! Reclaim bytes come from [`ReclaimEstimate`], so clone, snapshot, cloud and
//! dataless rules are applied exactly as everywhere else. This aggregator keeps one
//! record per file in memory; persisted, incremental aggregation arrives with the
//! store (roadmap 5.8–5.9).

use std::collections::{BTreeMap, BTreeSet, HashMap};

use lumen_domain::{ByteCount, EntryKind, FileIdentity, RawPath, ReclaimEstimate, SizeFacts};

use crate::ScanItem;

/// Totals for one directory's subtree (the directory itself excluded).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DirTotals {
    /// Distinct non-directory files under the directory.
    pub files: u64,
    /// Sum of logical sizes, once per file.
    pub logical: ByteCount,
    /// Sum of allocated sizes, once per file.
    pub allocated: ByteCount,
    /// Bytes freed immediately by removing the whole subtree.
    pub reclaimable_now: ByteCount,
    /// `false` when `reclaimable_now` is a lower bound.
    pub exact: bool,
}

impl Default for DirTotals {
    fn default() -> Self {
        Self {
            files: 0,
            logical: ByteCount::ZERO,
            allocated: ByteCount::ZERO,
            reclaimable_now: ByteCount::ZERO,
            exact: true,
        }
    }
}

/// Collects file entries from a scan and computes per-directory totals.
#[derive(Debug, Clone)]
pub struct SizeAggregator {
    roots: Vec<RawPath>,
    /// Per identity: the facts (as reported by the first link seen) and the paths
    /// of every link seen.
    files: HashMap<FileIdentity, (SizeFacts, Vec<RawPath>)>,
}

impl SizeAggregator {
    /// Starts aggregating for the given scan roots. Directories above the roots
    /// are never reported.
    pub fn new(roots: &[RawPath]) -> Self {
        Self {
            roots: roots.to_vec(),
            files: HashMap::new(),
        }
    }

    /// Records one scan result. Only non-directory entries carry sizes.
    pub fn observe(&mut self, item: &ScanItem) {
        let ScanItem::Entry(entry) = item else { return };
        if entry.kind == EntryKind::Directory {
            return;
        }
        self.files
            .entry(entry.size.identity.clone())
            .or_insert_with(|| (entry.size.clone(), Vec::new()))
            .1
            .push(entry.path.clone());
    }

    /// Computes totals for every directory under the roots that contains at least
    /// one file, keyed by directory path.
    pub fn finish(self) -> BTreeMap<RawPath, DirTotals> {
        let mut totals: BTreeMap<RawPath, DirTotals> = BTreeMap::new();
        for (facts, links) in self.files.values() {
            // Usage: every directory containing at least one link, once.
            let mut containing: BTreeSet<RawPath> = BTreeSet::new();
            for link in links {
                containing.extend(self.ancestors(link));
            }
            for dir in &containing {
                let t = totals.entry(dir.clone()).or_default();
                t.files += 1;
                t.logical = t.logical.saturating_add(facts.logical);
                t.allocated = t.allocated.saturating_add(facts.allocated);
            }

            // Reclaimable: only directories that contain *every* link.
            let all_links_seen =
                u32::try_from(links.len()).is_ok_and(|n| n >= facts.link_count.get());
            if !all_links_seen {
                continue;
            }
            let estimate = ReclaimEstimate::for_selection(std::iter::repeat_n(facts, links.len()));
            let common: Option<BTreeSet<RawPath>> = links
                .iter()
                .map(|link| self.ancestors(link).into_iter().collect::<BTreeSet<_>>())
                .reduce(|a, b| a.intersection(&b).cloned().collect());
            for dir in common.unwrap_or_default() {
                let t = totals.entry(dir).or_default();
                t.reclaimable_now = t.reclaimable_now.saturating_add(estimate.reclaimable_now);
                t.exact &= estimate.exact;
            }
        }
        totals
    }

    /// Directories containing `path`, from its parent up to (and including) the
    /// scan root it belongs to. Empty if the path is under no root.
    fn ancestors(&self, path: &RawPath) -> Vec<RawPath> {
        let Some(root) = self
            .roots
            .iter()
            .filter(|r| r.flavor() == path.flavor() && is_within(r.as_bytes(), path.as_bytes()))
            .max_by_key(|r| r.as_bytes().len())
        else {
            return Vec::new();
        };
        let mut out = Vec::new();
        let mut current = path.as_bytes();
        while current.len() > root.as_bytes().len() {
            let Some(cut) = current.iter().rposition(|b| *b == b'/' || *b == b'\\') else {
                break;
            };
            // Keep a lone leading separator ("/x" -> "/").
            current = if cut == 0 {
                &current[..1]
            } else {
                &current[..cut]
            };
            if current.len() < root.as_bytes().len() {
                break;
            }
            if let Ok(dir) = RawPath::from_stored(path.flavor(), current.to_vec()) {
                out.push(dir);
            }
        }
        out
    }
}

/// Whether `path` is strictly under `root`, comparing whole segments.
fn is_within(root: &[u8], path: &[u8]) -> bool {
    let Some(rest) = path.strip_prefix(root) else {
        return false;
    };
    !rest.is_empty()
        && (root.ends_with(b"/")
            || root.ends_with(b"\\")
            || rest.starts_with(b"/")
            || rest.starts_with(b"\\"))
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;

    use lumen_domain::{
        AccessState, EntryTimes, FileId, FilesystemEntry, Protection, SizeFlags, VolumeId,
    };

    use super::*;

    fn path(p: &str) -> RawPath {
        RawPath::from_unix_bytes(p.as_bytes().to_vec()).unwrap_or_else(|_| unreachable!())
    }

    fn file(p: &str, ino: u128, bytes: u64, links: u32) -> ScanItem {
        ScanItem::Entry(FilesystemEntry {
            path: path(p),
            kind: EntryKind::File,
            size: SizeFacts {
                identity: FileIdentity {
                    volume: VolumeId::new("v").unwrap_or_else(|_| unreachable!()),
                    file: FileId::new(ino),
                },
                logical: ByteCount::new(bytes),
                allocated: ByteCount::new(bytes),
                private: None,
                clone_id: None,
                link_count: NonZeroU32::new(links).unwrap_or(NonZeroU32::MIN),
                flags: SizeFlags::default(),
            },
            times: EntryTimes::default(),
            protection: Protection::default(),
            access: AccessState::Readable,
        })
    }

    fn totals(items: &[ScanItem]) -> BTreeMap<RawPath, DirTotals> {
        let mut agg = SizeAggregator::new(&[path("/r")]);
        for item in items {
            agg.observe(item);
        }
        agg.finish()
    }

    fn get(t: &BTreeMap<RawPath, DirTotals>, p: &str) -> DirTotals {
        t.get(&path(p)).copied().unwrap_or_default()
    }

    #[test]
    fn plain_files_roll_up_to_every_ancestor() {
        let t = totals(&[
            file("/r/a/x", 1, 100, 1),
            file("/r/a/b/y", 2, 10, 1),
            file("/r/z", 3, 1, 1),
        ]);
        assert_eq!(
            (get(&t, "/r/a/b").files, get(&t, "/r/a/b").allocated),
            (1, ByteCount::new(10))
        );
        assert_eq!(get(&t, "/r/a").allocated, ByteCount::new(110));
        assert_eq!(get(&t, "/r").allocated, ByteCount::new(111));
        assert_eq!(get(&t, "/r").reclaimable_now, ByteCount::new(111));
        assert!(
            !t.contains_key(&path("/")),
            "nothing above the root is reported"
        );
    }

    #[test]
    fn hard_links_count_once_and_are_reclaimable_only_where_all_links_live() {
        // Same file linked from two sibling directories.
        let t = totals(&[file("/r/a/x", 7, 100, 2), file("/r/b/x", 7, 100, 2)]);
        assert_eq!(
            get(&t, "/r").allocated,
            ByteCount::new(100),
            "counted once at the common ancestor"
        );
        assert_eq!(get(&t, "/r").reclaimable_now, ByteCount::new(100));
        for side in ["/r/a", "/r/b"] {
            assert_eq!(
                get(&t, side).allocated,
                ByteCount::new(100),
                "{side} uses the file"
            );
            assert_eq!(
                get(&t, side).reclaimable_now,
                ByteCount::ZERO,
                "{side} alone frees nothing"
            );
        }
    }

    #[test]
    fn links_outside_the_scan_make_a_file_reclaimable_nowhere() {
        let t = totals(&[file("/r/a/x", 7, 100, 3), file("/r/b/x", 7, 100, 3)]);
        assert_eq!(get(&t, "/r").allocated, ByteCount::new(100));
        assert_eq!(get(&t, "/r").reclaimable_now, ByteCount::ZERO);
    }

    #[test]
    fn shared_blocks_with_unknown_private_size_make_totals_inexact() {
        let mut item = file("/r/clone", 9, 500, 1);
        if let ScanItem::Entry(e) = &mut item {
            e.size.flags.may_share_blocks = true;
        }
        let t = totals(&[item, file("/r/plain", 10, 5, 1)]);
        assert_eq!(get(&t, "/r").reclaimable_now, ByteCount::new(5));
        assert!(!get(&t, "/r").exact);
    }

    #[test]
    fn segment_boundaries_are_respected() {
        let mut agg = SizeAggregator::new(&[path("/r")]);
        agg.observe(&file("/r2/x", 1, 50, 1));
        assert!(agg.finish().is_empty(), "/r2 is not under /r");
    }

    #[test]
    fn root_slash_is_supported() {
        let mut agg = SizeAggregator::new(&[path("/")]);
        agg.observe(&file("/x", 1, 50, 1));
        agg.observe(&file("/d/y", 2, 5, 1));
        let t = agg.finish();
        assert_eq!(get(&t, "/").allocated, ByteCount::new(55));
        assert_eq!(get(&t, "/d").allocated, ByteCount::new(5));
    }

    use proptest::prelude::*;

    proptest! {
        #[test]
        fn totals_are_consistent_up_the_tree(
            files in prop::collection::vec((0usize..4, 0usize..4, 0u128..12, 1u32..4), 1..40),
        ) {
            // Paths /r/dA/dB/fN; identities shared across paths model hard links.
            // Size and link count derive from the identity, as a real scanner
            // reports them consistently for every link of one file.
            let items: Vec<ScanItem> = files
                .iter()
                .enumerate()
                .map(|(n, (a, b, ino, _))| {
                    let size = (u64::try_from(*ino).unwrap_or(0) + 1) * 10;
                    let links = files.iter().find(|f| f.2 == *ino).map_or(1, |f| f.3);
                    file(&format!("/r/d{a}/d{b}/f{n}"), *ino, size, links)
                })
                .collect();
            let t = totals(&items);
            for (dir, totals) in &t {
                prop_assert!(totals.reclaimable_now <= totals.allocated, "{dir:?}");
                // Every ancestor's totals cover its descendants' totals.
                let bytes = dir.as_bytes();
                if let Some(cut) = bytes.iter().rposition(|b| *b == b'/')
                    && cut > 0
                    && let Some(parent) = t.get(&path(&String::from_utf8_lossy(&bytes[..cut])))
                {
                    prop_assert!(parent.allocated >= totals.allocated);
                    prop_assert!(parent.reclaimable_now >= totals.reclaimable_now);
                    prop_assert!(parent.files >= totals.files);
                }
            }
        }
    }
}
