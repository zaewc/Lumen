//! The size of a folder tree in a stored scan (roadmap 16.2): summed in SQL,
//! so a bundle with hundreds of thousands of files is measured without
//! reading its entries into memory.

use lumen_domain::{
    AccessState, ByteCount, EntryKind, PathFlavor, RawPath, ReclaimEstimate, ScanId,
};
use rusqlite::{OptionalExtension, params};

use super::{IndexStore, access_text, dirs, flavor_text, from_i64, kind_text};
use crate::StoreError;

/// What a folder tree held in a scan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SubtreeSize {
    /// What removing the whole tree would free, estimated exactly as
    /// [`ReclaimEstimate::for_selection`] does for its entries.
    pub reclaim: ReclaimEstimate,
    /// Every folder in the tree, the top one included, was listed in full
    /// and readably. Otherwise the sizes are lower bounds: unread is never
    /// empty (ADR-0008).
    pub complete: bool,
}

/// The entries at and below a folder: the folder's own entry (`?3`, `?4`,
/// only while `?7` is 1), then a range of directory paths (`?5`, `?6`). Both
/// are index seeks, as in [`IndexStore::entries_within`].
const SELECTED: &str = "
    WITH sel AS (
        SELECT e.dir, e.name, e.kind, e.volume, e.file_id, e.logical, e.allocated,
               e.private, e.link_count, e.size_flags
        FROM dirs d CROSS JOIN entries e ON e.dir = d.id
        WHERE ?7 = 1 AND d.scan = (SELECT key FROM scans WHERE id = ?1)
          AND d.flavor = ?2 AND d.path = ?3 AND e.name = ?4
        UNION ALL
        SELECT e.dir, e.name, e.kind, e.volume, e.file_id, e.logical, e.allocated,
               e.private, e.link_count, e.size_flags
        FROM dirs d CROSS JOIN entries e ON e.dir = d.id
        WHERE d.scan = (SELECT key FROM scans WHERE id = ?1)
          AND d.flavor = ?2 AND d.path >= ?5 AND d.path < ?6
    ),
    files AS (
        SELECT count(*) AS links, max(logical) AS logical, max(allocated) AS allocated,
               max(private) AS private, max(link_count) AS link_count,
               max(size_flags) AS flags
        FROM sel GROUP BY volume, file_id
    )";

/// The totals over `files`, mirroring `SizeFacts::whole_file_reclaim`: a
/// file counts only when every one of its links is in the tree; a cloud
/// placeholder or dataless file (flags 8 and 4) frees nothing; a known
/// private size frees at most the allocated size; shared blocks (flag 16)
/// with no private size free nothing promised and make the total inexact.
/// Then whether the top folder's own entry exists, looked up on its own even
/// when the range holds it (a root), and how many folders in the tree were
/// not listed in full (`?8` directory, `?9` readable).
const TOTALS: &str = "
    SELECT
        (SELECT count(*) FROM files),
        (SELECT coalesce(sum(logical), 0) FROM files),
        (SELECT coalesce(sum(allocated), 0) FROM files),
        (SELECT coalesce(sum(CASE
            WHEN links < link_count THEN 0
            WHEN flags & 12 != 0 THEN 0
            WHEN private IS NOT NULL THEN min(private, allocated)
            WHEN flags & 16 != 0 THEN 0
            ELSE allocated END), 0) FROM files),
        (SELECT count(*) FROM files WHERE links >= link_count AND flags & 12 = 0
            AND private IS NULL AND flags & 16 != 0),
        (SELECT count(*) FROM files WHERE links < link_count),
        (SELECT count(*) FROM dirs d CROSS JOIN entries e ON e.dir = d.id
            WHERE d.scan = (SELECT key FROM scans WHERE id = ?1)
              AND d.flavor = ?2 AND d.path = ?3 AND e.name = ?4),
        (SELECT count(*) FROM sel s LEFT JOIN listings l ON l.dir = s.dir AND l.name = s.name
            WHERE s.kind = ?8 AND (l.dir IS NULL OR l.complete = 0 OR l.access != ?9))";

impl IndexStore {
    /// The size of `dir` and everything below it (whole segments, as
    /// [`Self::entries_within`]) in `scan`, or `None` when the scan has no
    /// entry for `dir` itself: it lies outside the scan's roots, in a folder
    /// the scan could not read, or appeared after it. Never a zero for
    /// unknown.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] if SQLite fails or a total is out of range.
    pub fn subtree_size(
        &self,
        scan: ScanId,
        dir: &RawPath,
    ) -> Result<Option<SubtreeSize>, StoreError> {
        // `/a/b/` is `/a/b`: its own entry is stored under its last name.
        let trimmed = trailing_separator_trimmed(dir);
        let dir = trimmed.as_ref().unwrap_or(dir);
        let (start, end) = dirs::below_range(dir);
        let (own_dir, own_name) = dirs::split(dir);
        // The range already holds `dir`'s own entry when `dir` is a root
        // such as `/`; it is then not selected a second time.
        let own_apart = !own_dir.starts_with(&start);
        let mut query = self.conn.prepare_cached(&format!("{SELECTED} {TOTALS}"))?;
        let row = query
            .query_row(
                params![
                    scan.to_string(),
                    flavor_text(dir.flavor()),
                    own_dir,
                    own_name,
                    start,
                    end,
                    i64::from(own_apart),
                    kind_text(EntryKind::Directory),
                    access_text(AccessState::Readable)?,
                ],
                |row| {
                    Ok([
                        row.get::<_, i64>(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                        row.get(6)?,
                        row.get(7)?,
                    ])
                },
            )
            .optional()?;
        let Some(
            [
                files,
                logical,
                allocated,
                now,
                inexact,
                partial,
                own,
                unlisted,
            ],
        ) = row
        else {
            return Ok(None);
        };
        if own == 0 {
            return Ok(None);
        }
        Ok(Some(SubtreeSize {
            reclaim: ReclaimEstimate {
                logical: ByteCount::new(from_i64("logical", logical)?),
                allocated: ByteCount::new(from_i64("allocated", allocated)?),
                reclaimable_now: ByteCount::new(from_i64("private", now)?),
                exact: inexact == 0,
                files: from_i64("files", files)?,
                partially_linked_files: from_i64("link_count", partial)?,
            },
            complete: unlisted == 0,
        }))
    }
}

/// `dir` without its trailing separators, unless it is a root, whose own
/// entry is stored with one (`/`, `C:\`, `\\server\share\`); `None` when
/// there is nothing to trim. A root this does not recognize stays trimmed
/// and is then not found: `None`, never a wrong size.
fn trailing_separator_trimmed(dir: &RawPath) -> Option<RawPath> {
    let separator = dirs::separator(dir.flavor());
    let bytes = dir.as_bytes();
    let end = bytes
        .iter()
        .rposition(|&b| b != separator)
        .map_or(1, |last| last + 1);
    let kept = &bytes[..end];
    if end >= bytes.len() || (dir.flavor() == PathFlavor::Windows && windows_root(kept)) {
        return None;
    }
    RawPath::from_stored(dir.flavor(), kept.to_vec()).ok()
}

/// Whether a Windows path without its trailing `\` names a root: a drive
/// (`C:`, `\\?\C:`) or a share (`\\server\share`).
fn windows_root(kept: &[u8]) -> bool {
    kept.ends_with(b":")
        || kept.strip_prefix(b"\\\\").is_some_and(|rest| {
            let mut at = rest.iter().filter(|&&b| b == b'\\');
            at.next().is_some() && at.next().is_none()
        })
}

#[cfg(test)]
mod tests {
    use lumen_application::ports::DirOutcome;
    use lumen_domain::{
        AccessState, ByteCount, CloneId, EntryKind, FilesystemEntry, PathFlavor, RawPath,
        ReclaimEstimate, Timestamp,
    };
    use proptest::prelude::*;
    use rusqlite::params;

    use super::super::tests::{entry, scan_id, store};
    use super::{SELECTED, SubtreeSize, TOTALS};

    fn raw(p: &[u8]) -> RawPath {
        RawPath::from_unix_bytes(p.to_vec()).unwrap_or_else(|_| unreachable!())
    }

    fn folder(path: &[u8], file: u128) -> FilesystemEntry {
        let mut e = entry(path, file);
        e.kind = EntryKind::Directory;
        e
    }

    const READ: DirOutcome = DirOutcome {
        access: AccessState::Readable,
        complete: true,
    };

    /// What [`ReclaimEstimate::for_selection`] says for the tree's entries.
    fn expected(entries: &[FilesystemEntry]) -> ReclaimEstimate {
        ReclaimEstimate::for_selection(entries.iter().map(|e| &e.size))
    }

    #[test]
    fn a_bundle_is_summed_like_the_preview_and_complete_only_when_read()
    -> Result<(), Box<dyn std::error::Error>> {
        let (_fx, mut store) = store()?;
        let scan = scan_id(1);
        store.begin_scan(scan, Timestamp::UNIX_EPOCH, &[])?;
        let mut linked = entry(b"/Applications/X.app/Contents/a", 3);
        linked.size.link_count = 2.try_into()?;
        let mut outside = linked.clone();
        outside.path = raw(b"/Applications/Y/a");
        let mut shared = entry(b"/Applications/X.app/Contents/b", 4);
        shared.size.flags.may_share_blocks = true;
        shared.size.clone_id = Some(CloneId::new(9));
        let mut cloud = entry(b"/Applications/X.app/Contents/c", 5);
        cloud.size.flags.cloud_placeholder = true;
        let mut private = entry(b"/Applications/X.app/Contents/d", 6);
        private.size.private = Some(ByteCount::new(100));
        let bundle = vec![
            folder(b"/Applications/X.app", 1),
            folder(b"/Applications/X.app/Contents", 2),
            linked,
            shared,
            cloud,
            private,
        ];
        let mut all = bundle.clone();
        all.extend([
            folder(b"/Applications", 10),
            folder(b"/Applications/X.apps", 11),
            entry(b"/Applications/X.apps/z", 12),
            outside,
        ]);
        store.insert_entries(scan, &all)?;
        store.insert_listings(
            scan,
            &[
                (raw(b"/Applications"), READ),
                (raw(b"/Applications/X.app"), READ),
            ],
        )?;

        let size = store.subtree_size(scan, &raw(b"/Applications/X.app"))?;
        let estimate = expected(&bundle);
        assert_eq!(
            size,
            Some(SubtreeSize {
                reclaim: estimate,
                complete: false
            }),
            "Contents was not listed"
        );
        assert!(!estimate.exact, "shared blocks with no private size");
        assert_eq!(estimate.partially_linked_files, 1);

        store.insert_listings(scan, &[(raw(b"/Applications/X.app/Contents"), READ)])?;
        let size = store.subtree_size(scan, &raw(b"/Applications/X.app/"))?;
        assert_eq!(size.map(|s| s.complete), Some(true));
        assert_eq!(size.map(|s| s.reclaim), Some(estimate), "a trailing slash");

        let cut = DirOutcome {
            access: AccessState::Readable,
            complete: false,
        };
        store.insert_listings(scan, &[(raw(b"/Applications/X.app/Contents"), cut)])?;
        let size = store.subtree_size(scan, &raw(b"/Applications/X.app"))?;
        assert_eq!(size.map(|s| s.complete), Some(false), "cut short");

        assert_eq!(
            store.subtree_size(scan, &raw(b"/Applications/W.app"))?,
            None
        );
        assert_eq!(
            store.subtree_size(scan_id(2), &raw(b"/Applications/X.app"))?,
            None,
            "another scan"
        );
        assert_eq!(
            store.subtree_size(scan, &raw(b"/"))?,
            None,
            "the scan has no entry for / itself"
        );
        store.insert_entries(scan, &[folder(b"/", 20)])?;
        let whole = store.subtree_size(scan, &raw(b"/"))?;
        assert_eq!(
            whole.map(|s| (s.reclaim.files, s.reclaim.partially_linked_files)),
            Some((10, 0)),
            "/ holds the rest; the two links of one file count once, both inside"
        );
        Ok(())
    }

    fn windows(p: &str) -> RawPath {
        RawPath::from_stored(PathFlavor::Windows, p.as_bytes().to_vec())
            .unwrap_or_else(|_| unreachable!())
    }

    fn at(path: RawPath, mut e: FilesystemEntry) -> FilesystemEntry {
        e.path = path;
        e
    }

    #[test]
    fn a_windows_folder_is_found_with_a_trailing_separator_and_roots_keep_theirs()
    -> Result<(), Box<dyn std::error::Error>> {
        let (_fx, mut store) = store()?;
        let scan = scan_id(4);
        store.begin_scan(scan, Timestamp::UNIX_EPOCH, &[])?;
        store.insert_entries(
            scan,
            &[
                at(windows(r"C:\"), folder(b"/", 1)),
                at(windows(r"C:\empty"), folder(b"/", 2)),
                at(windows(r"C:\unread"), folder(b"/", 3)),
                at(windows(r"C:\unread\f"), entry(b"/", 4)),
            ],
        )?;
        store.insert_listings(scan, &[(windows(r"C:\empty"), READ)])?;

        let empty = store.subtree_size(scan, &windows(r"C:\empty\"))?;
        assert_eq!(
            empty.map(|s| (s.reclaim.files, s.complete)),
            Some((1, true)),
            "an empty folder, asked for with a trailing separator"
        );
        let unread = store.subtree_size(scan, &windows(r"C:\unread\"))?;
        assert_eq!(
            unread.map(|s| (s.reclaim.files, s.complete)),
            Some((2, false)),
            "a top folder with no listing is never complete"
        );
        let root = store.subtree_size(scan, &windows(r"C:\"))?;
        assert_eq!(root.map(|s| s.reclaim.files), Some(4), "the drive root");
        Ok(())
    }

    #[test]
    fn only_non_root_trailing_separators_are_trimmed() {
        let trimmed = |p: RawPath| super::trailing_separator_trimmed(&p).map(|t| t.display());
        assert_eq!(trimmed(windows(r"C:\a\\")), Some(r"C:\a".to_owned()));
        assert_eq!(trimmed(windows(r"C:\")), None);
        assert_eq!(trimmed(windows(r"\\?\C:\")), None);
        assert_eq!(trimmed(windows(r"\\srv\share\")), None);
        assert_eq!(
            trimmed(windows(r"\\srv\share\a\")),
            Some(r"\\srv\share\a".to_owned())
        );
        assert_eq!(trimmed(raw(b"/a//")), Some("/a".to_owned()));
        assert_eq!(trimmed(raw(b"//")), Some("/".to_owned()));
        assert_eq!(trimmed(raw(b"/")), None);
    }

    #[test]
    fn a_tree_is_summed_by_index_seeks() -> Result<(), Box<dyn std::error::Error>> {
        let (_fx, store) = store()?;
        let sql = format!("EXPLAIN QUERY PLAN {SELECTED} {TOTALS}");
        let plan = store
            .conn
            .prepare(&sql)?
            .query_map(
                params![
                    "s",
                    "unix",
                    b"/a/".as_slice(),
                    b"b".as_slice(),
                    b"/a/b/".as_slice(),
                    b"/a/b0".as_slice(),
                    1,
                    "directory",
                    "\"readable\""
                ],
                |row| row.get::<_, String>(3),
            )?
            .collect::<Result<Vec<_>, _>>()?
            .join("\n");
        for seek in [
            "(scan=? AND flavor=? AND path=?)",
            "(scan=? AND flavor=? AND path>? AND path<?)",
            "SEARCH e USING PRIMARY KEY (dir=? AND name=?)",
            "SEARCH e USING PRIMARY KEY (dir=?)",
            "SEARCH l USING PRIMARY KEY (dir=? AND name=?)",
        ] {
            assert!(plan.contains(seek), "{seek}:\n{plan}");
        }
        for scan in ["SCAN d", "SCAN e", "SCAN l", "SCAN entries", "SCAN dirs"] {
            assert!(!plan.contains(scan), "{scan}:\n{plan}");
        }
        Ok(())
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(64))]

        /// Any tree, with links, shared blocks, placeholders and private
        /// sizes, sums to what the preview's estimate says for its entries.
        #[test]
        fn sql_totals_match_the_estimate(
            files in prop::collection::vec(
                (0u8..4, 0u8..12, 1u32..4, 0u64..1 << 40, prop::option::of(0u64..1 << 40), 0i64..32),
                0..24,
            )
        ) {
            let check = || -> Result<(), Box<dyn std::error::Error>> {
                let (_fx, mut store) = store()?;
                let scan = scan_id(3);
                store.begin_scan(scan, Timestamp::UNIX_EPOCH, &[])?;
                let top = folder(b"/t", 1000);
                let mut inside = vec![top.clone()];
                let mut rows = vec![top];
                for (n, (place, file, links, allocated, private, flags)) in files.iter().enumerate() {
                    let path = match place {
                        0 | 1 => format!("/t/f{n}"),
                        2 => format!("/t/s/f{n}"),
                        _ => format!("/u/f{n}"),
                    };
                    let mut e = entry(path.as_bytes(), u128::from(*file));
                    // The same file has the same facts through every link.
                    let seed = u64::from(*file);
                    e.size.link_count = (*links).max(1).try_into()?;
                    e.size.allocated = ByteCount::new(allocated % (seed + 1) * 4096);
                    e.size.logical = ByteCount::new(seed * 7);
                    e.size.private = private.map(|p| ByteCount::new(p % (seed + 2)));
                    e.size.flags = super::super::size_flags_from(*flags);
                    let first = rows.iter().find(|r| r.size.identity == e.size.identity).cloned();
                    if let Some(first) = first {
                        e.size = first.size;
                    }
                    if *place < 3 {
                        inside.push(e.clone());
                    }
                    rows.push(e);
                }
                store.insert_entries(scan, &rows)?;
                let size = store.subtree_size(scan, &raw(b"/t"))?;
                let (got, want) = (size.map(|s| s.reclaim), Some(expected(&inside)));
                if got != want {
                    return Err(format!("{got:?} != {want:?}").into());
                }
                Ok(())
            };
            check().map_err(|e| TestCaseError::fail(e.to_string()))?;
        }
    }
}
