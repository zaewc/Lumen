//! Directories and names (schema v6, roadmap 13.c).
//!
//! A stored path is split after its last separator into a directory path,
//! kept once per scan in `dirs`, and a name: `/Users/ana` is `/Users/` and
//! `ana`, the root `/` is `/` and an empty name (so is `C:\` on Windows), and a
//! path without a separator is an empty directory path and the whole path as
//! its name. Joining the two gives the path back byte for byte. The separator
//! is the flavor's, as [`lumen_application::observe::below`] splits: `/` for
//! Unix and `\` for Windows paths (WTF-8 never uses that byte
//! inside a multi-byte sequence, so the halves stay well formed).

use std::collections::HashMap;

use lumen_domain::{PathFlavor, RawPath, ScanId};
use rusqlite::{OptionalExtension as _, Transaction, params};

use super::{flavor_text, prefix_end};
use crate::StoreError;

/// Directory ids the writer remembers before it starts over; a scan lists
/// one directory's entries together, so recent directories are what repeat.
const CACHE_LIMIT: usize = 16_384;

/// The separator `below` splits on for `flavor`.
pub(super) const fn separator(flavor: PathFlavor) -> u8 {
    match flavor {
        PathFlavor::Unix => b'/',
        PathFlavor::Windows => b'\\',
    }
}

/// `path` as (directory path, name): split after the last separator.
pub(super) fn split(path: &RawPath) -> (&[u8], &[u8]) {
    let bytes = path.as_bytes();
    let at = bytes
        .iter()
        .rposition(|&b| b == separator(path.flavor()))
        .map_or(0, |i| i + 1);
    bytes.split_at(at)
}

/// The directory paths holding everything strictly below `dir`, as a
/// half-open byte range: `dir` with one trailing separator, up to the next
/// byte string after every string with that prefix.
pub(super) fn below_range(dir: &RawPath) -> (Vec<u8>, Vec<u8>) {
    let separator = separator(dir.flavor());
    let bytes = dir.as_bytes();
    let mut start = bytes.strip_suffix(&[separator]).unwrap_or(bytes).to_vec();
    start.push(separator);
    // The prefix ends with a separator (below 0xFF), so the end always exists.
    let end = prefix_end(&start).unwrap_or_default();
    (start, end)
}

/// The writer's directory ids for one scan, so most rows insert without a
/// lookup. Bounded: it starts over after [`CACHE_LIMIT`] directories, and it
/// is cleared whenever a write fails, since a rolled-back transaction takes
/// its new directories with it.
#[derive(Debug, Default)]
pub(super) struct DirCache {
    /// The scan the ids belong to, and its key.
    scan: Option<(ScanId, i64)>,
    unix: HashMap<Vec<u8>, i64>,
    windows: HashMap<Vec<u8>, i64>,
}

impl DirCache {
    pub(super) fn clear(&mut self) {
        self.scan = None;
        self.unix.clear();
        self.windows.clear();
    }

    fn ids(&mut self, flavor: PathFlavor) -> &mut HashMap<Vec<u8>, i64> {
        match flavor {
            PathFlavor::Unix => &mut self.unix,
            PathFlavor::Windows => &mut self.windows,
        }
    }

    /// The integer key of `scan`.
    ///
    /// # Errors
    ///
    /// [`StoreError::UnknownScan`] if the scan does not exist,
    /// [`StoreError::Corrupt`] if it has no valid key.
    pub(super) fn scan_key(
        &mut self,
        tx: &Transaction<'_>,
        scan: ScanId,
    ) -> Result<i64, StoreError> {
        if let Some((cached, key)) = self.scan
            && cached == scan
        {
            return Ok(key);
        }
        self.clear();
        let key: Option<Option<i64>> = tx
            .prepare_cached("SELECT key FROM scans WHERE id = ?1")?
            .query_row([scan.to_string()], |row| row.get(0))
            .optional()?;
        let key = key
            .ok_or(StoreError::UnknownScan { scan })?
            .ok_or(StoreError::Corrupt {
                column: "scans.key",
            })?;
        self.scan = Some((scan, key));
        Ok(key)
    }

    /// The id of directory `path` in the scan keyed `scan`, added if new.
    pub(super) fn id(
        &mut self,
        tx: &Transaction<'_>,
        scan: i64,
        flavor: PathFlavor,
        path: &[u8],
    ) -> Result<i64, StoreError> {
        if let Some(&id) = self.ids(flavor).get(path) {
            return Ok(id);
        }
        let flavor_text = flavor_text(flavor);
        let found: Option<i64> = tx
            .prepare_cached("SELECT id FROM dirs WHERE scan = ?1 AND flavor = ?2 AND path = ?3")?
            .query_row(params![scan, flavor_text, path], |row| row.get(0))
            .optional()?;
        let id = if let Some(id) = found {
            id
        } else {
            tx.prepare_cached("INSERT INTO dirs (scan, flavor, path) VALUES (?1, ?2, ?3)")?
                .execute(params![scan, flavor_text, path])?;
            tx.last_insert_rowid()
        };
        let ids = self.ids(flavor);
        if ids.len() >= CACHE_LIMIT {
            ids.clear();
        }
        ids.insert(path.to_vec(), id);
        Ok(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unix(p: &[u8]) -> RawPath {
        RawPath::from_unix_bytes(p.to_vec()).unwrap_or_else(|_| unreachable!())
    }

    fn windows(p: &str) -> RawPath {
        let wide: Vec<u16> = p.encode_utf16().collect();
        RawPath::from_windows_wide(&wide).unwrap_or_else(|_| unreachable!())
    }

    #[test]
    fn paths_split_after_the_flavors_last_separator() {
        for (path, dir, name) in [
            (unix(b"/Users/ana"), &b"/Users/"[..], &b"ana"[..]),
            (unix(b"/Users/ana/"), b"/Users/ana/", b""),
            (unix(b"/a"), b"/", b"a"),
            (unix(b"/"), b"/", b""),
            (unix(b"relative"), b"", b"relative"),
            (unix(b""), b"", b""),
            (unix(b"/a\\b"), b"/", b"a\\b"),
            (windows(r"C:\Users\ana"), br"C:\Users\", b"ana"),
            (windows(r"C:\"), br"C:\", b""),
            (windows(r"C:"), b"", b"C:"),
            (windows(r"\\?\C:\x"), br"\\?\C:\", b"x"),
            (windows("C:/a/b"), b"", b"C:/a/b"),
            (
                windows("C:\\\u{e9}t\u{e9}\\\u{1F600}"),
                "C:\\\u{e9}t\u{e9}\\".as_bytes(),
                "\u{1F600}".as_bytes(),
            ),
        ] {
            let (d, n) = split(&path);
            assert_eq!((d, n), (dir, name), "{}", path.display());
            assert_eq!([d, n].concat(), path.as_bytes(), "joins back");
            let rebuilt = RawPath::from_stored(path.flavor(), [d, n].concat());
            assert_eq!(rebuilt.ok(), Some(path));
        }
    }

    #[test]
    fn below_ranges_cover_whole_segments() {
        for (dir, start, end) in [
            (unix(b"/a/b"), &b"/a/b/"[..], &b"/a/b0"[..]),
            (unix(b"/a/b/"), b"/a/b/", b"/a/b0"),
            (unix(b"/"), b"/", b"0"),
            (windows(r"C:\"), br"C:\", b"C:]"),
            (windows(r"C:\x"), br"C:\x\", b"C:\\x]"),
        ] {
            let (s, e) = below_range(&dir);
            assert_eq!(
                (s.as_slice(), e.as_slice()),
                (start, end),
                "{}",
                dir.display()
            );
        }
    }
}
