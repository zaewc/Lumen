//! Reading and writing scan results in `index.db`.
//!
//! Writes are batched: one transaction per call, so a scan's entries go in at
//! SQLite's single-writer pace without per-row commits (ADR-0012). Reads
//! re-validate every column through the domain constructors, so a corrupted or
//! tampered row is reported as an error naming the column, never returned as a
//! silently wrong entry.

use std::num::NonZeroU32;

use lumen_application::ports::{DirOutcome, PendingDirectory};
use lumen_domain::{
    AccessState, ByteCount, CloneId, EntryKind, EntryTimes, FileId, FileIdentity, FilesystemEntry,
    PathFlavor, Protection, RawPath, ScanId, ScanState, SizeFacts, SizeFlags, SnapshotId,
    Timestamp, VolumeId,
};
use rusqlite::{Connection, OptionalExtension, Row, params};

use crate::StoreError;

/// Totals for one scan root at the end of a scan: one point in its history.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RootSnapshot {
    /// The scan root.
    pub root: RawPath,
    /// When the scan finished.
    pub taken_at: Timestamp,
    /// Distinct files under the root.
    pub files: u64,
    /// Logical bytes, once per file.
    pub logical: ByteCount,
    /// Allocated bytes, once per file.
    pub allocated: ByteCount,
    /// Bytes removable from the whole root right now.
    pub reclaimable: ByteCount,
    /// Whether `reclaimable` is exact rather than a lower bound.
    pub exact: bool,
    /// Whether the scan observed everything under the root (coverage complete).
    pub complete: bool,
}

/// Scan results in `index.db`.
///
/// Paths are stored as a directory, kept once per scan, plus a name (schema
/// v6), so whole-scan reads return rows ordered by flavor, then directory path
/// bytes, then name bytes: a directory's own entries come before its
/// subdirectories' (`/a/bc` before `/a/b/c`).
#[derive(Debug)]
pub struct IndexStore {
    conn: Connection,
    /// Directory ids of the scan being written.
    dirs: DirCache,
}

mod dirs;
mod graph;
mod plans;
mod prune;
#[cfg(test)]
mod size;

use dirs::DirCache;
pub use plans::StoredPlan;
pub use prune::Pruned;

/// An entry query for scan `?1`: its directory and name first, then the
/// columns [`entry_from_row`] reads. `$filter` narrows it, `$tail` follows the
/// ordering (a `LIMIT`). The scan's directories drive the join in path order,
/// so the ordering comes from the indexes, never from a sort.
macro_rules! select_entries {
    ($filter:literal, $tail:literal) => {
        concat!(
            "SELECT d.flavor, d.path, e.name, e.volume, e.file_id, e.kind, e.logical, e.allocated, e.private,
                    e.clone_id, e.link_count, e.size_flags, e.protection, e.modified_secs, e.modified_nanos,
                    e.accessed_secs, e.accessed_nanos, e.changed_secs, e.changed_nanos, e.created_secs,
                    e.created_nanos, e.access
             FROM dirs d CROSS JOIN entries e ON e.dir = d.id
             WHERE d.scan = (SELECT key FROM scans WHERE id = ?1) ",
            $filter,
            " ORDER BY d.flavor, d.path, e.name ",
            $tail
        )
    };
}

/// A listing query for scan `?1`, as [`select_entries`] for entries.
macro_rules! select_listings {
    ($filter:literal) => {
        concat!(
            "SELECT d.flavor, d.path, l.name, l.access, l.complete
             FROM dirs d CROSS JOIN listings l ON l.dir = d.id
             WHERE d.scan = (SELECT key FROM scans WHERE id = ?1) ",
            $filter,
            " ORDER BY d.flavor, d.path, l.name"
        )
    };
}

impl IndexStore {
    /// Wraps a connection opened with [`open_index`](crate::open_index).
    pub fn new(conn: Connection) -> Self {
        Self {
            conn,
            dirs: DirCache::default(),
        }
    }

    /// Records the start of a scan and its roots.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] if the scan already exists or SQLite fails.
    pub fn begin_scan(
        &mut self,
        scan: ScanId,
        started_at: Timestamp,
        roots: &[RawPath],
    ) -> Result<(), StoreError> {
        let tx = self.conn.transaction()?;
        tx.execute(
            "INSERT INTO scans (id, started_secs, started_nanos, state, key)
             VALUES (?1, ?2, ?3, 'running', (SELECT coalesce(max(key), 0) + 1 FROM scans))",
            params![
                scan.to_string(),
                started_at.unix_seconds(),
                started_at.subsec_nanos()
            ],
        )?;
        {
            let mut insert = tx.prepare_cached(
                "INSERT INTO scan_roots (scan_id, flavor, path) VALUES (?1, ?2, ?3)",
            )?;
            for root in roots {
                insert.execute(params![
                    scan.to_string(),
                    flavor_text(root.flavor()),
                    root.as_bytes()
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Inserts a batch of entries in one transaction. All or nothing. An entry
    /// already stored for the same path is replaced by the newer observation,
    /// which makes resuming an interrupted scan idempotent.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError::UnknownScan`] if the scan was never begun, or
    /// [`StoreError`] if a value is out of SQLite's range or SQLite fails; no
    /// entry of the batch is stored then.
    pub fn insert_entries(
        &mut self,
        scan: ScanId,
        entries: &[FilesystemEntry],
    ) -> Result<(), StoreError> {
        let written = self.write_entries(scan, entries);
        if written.is_err() {
            // The rollback also removed directories this batch added.
            self.dirs.clear();
        }
        written
    }

    fn write_entries(
        &mut self,
        scan: ScanId,
        entries: &[FilesystemEntry],
    ) -> Result<(), StoreError> {
        let tx = self.conn.transaction()?;
        let key = self.dirs.scan_key(&tx, scan)?;
        {
            let mut insert = tx.prepare_cached(
                "INSERT OR REPLACE INTO entries (dir, name, volume, file_id, kind, logical, allocated, private, clone_id,
                                      link_count, size_flags, protection, modified_secs, modified_nanos, accessed_secs,
                                      accessed_nanos, changed_secs, changed_nanos, created_secs, created_nanos, access)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21)",
            )?;
            for entry in entries {
                let (dir, name) = dirs::split(&entry.path);
                let dir = self.dirs.id(&tx, key, entry.path.flavor(), dir)?;
                let size = &entry.size;
                let (modified, accessed, changed, created) = (
                    split(entry.times.modified),
                    split(entry.times.accessed),
                    split(entry.times.changed),
                    split(entry.times.created),
                );
                insert.execute(params![
                    dir,
                    name,
                    size.identity.volume.as_str(),
                    size.identity.file.get().to_be_bytes(),
                    kind_text(entry.kind),
                    to_i64("logical", size.logical.get())?,
                    to_i64("allocated", size.allocated.get())?,
                    size.private
                        .map(|p| to_i64("private", p.get()))
                        .transpose()?,
                    size.clone_id.map(|c| c.get().to_be_bytes()),
                    i64::from(size.link_count.get()),
                    size_flag_bits(size.flags),
                    protection_bits(entry.protection),
                    modified.0,
                    modified.1,
                    accessed.0,
                    accessed.1,
                    changed.0,
                    changed.1,
                    created.0,
                    created.1,
                    access_text(entry.access)?,
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Inserts a batch of directory listing outcomes in one transaction. A
    /// listing already stored for the same path is replaced.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError::UnknownScan`] if the scan was never begun, or
    /// [`StoreError`] if SQLite fails; no row of the batch is stored then.
    pub fn insert_listings(
        &mut self,
        scan: ScanId,
        listings: &[(RawPath, DirOutcome)],
    ) -> Result<(), StoreError> {
        let written = self.write_listings(scan, listings);
        if written.is_err() {
            self.dirs.clear();
        }
        written
    }

    fn write_listings(
        &mut self,
        scan: ScanId,
        listings: &[(RawPath, DirOutcome)],
    ) -> Result<(), StoreError> {
        let tx = self.conn.transaction()?;
        let key = self.dirs.scan_key(&tx, scan)?;
        {
            let mut insert = tx.prepare_cached(
                "INSERT OR REPLACE INTO listings (dir, name, access, complete) VALUES (?1, ?2, ?3, ?4)",
            )?;
            for (path, outcome) in listings {
                let (dir, name) = dirs::split(path);
                let dir = self.dirs.id(&tx, key, path.flavor(), dir)?;
                insert.execute(params![
                    dir,
                    name,
                    access_text(outcome.access)?,
                    outcome.complete
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Records how a scan ended.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError::UnknownScan`] if the scan was never begun, or
    /// [`StoreError`] if SQLite fails.
    pub fn finish_scan(
        &mut self,
        scan: ScanId,
        state: ScanState,
        finished_at: Timestamp,
    ) -> Result<(), StoreError> {
        let changed = self.conn.execute(
            "UPDATE scans SET state = ?2, finished_secs = ?3, finished_nanos = ?4 WHERE id = ?1",
            params![
                scan.to_string(),
                scan_state_text(state),
                finished_at.unix_seconds(),
                finished_at.subsec_nanos()
            ],
        )?;
        if changed == 0 {
            return Err(StoreError::UnknownScan { scan });
        }
        Ok(())
    }

    /// The recorded state of a scan, if it exists.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] if SQLite fails or the stored state is invalid.
    pub fn scan_state(&self, scan: ScanId) -> Result<Option<ScanState>, StoreError> {
        let text: Option<String> = self
            .conn
            .query_row(
                "SELECT state FROM scans WHERE id = ?1",
                [scan.to_string()],
                |row| row.get(0),
            )
            .optional()?;
        text.map(|t| parse_scan_state(&t)).transpose()
    }

    /// The most recently started scan that completed, if any (roadmap 10.13).
    /// Cancelled, failed and unfinished scans are never assessed: what they
    /// did not read is unknown, not absent (ADR-0008).
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] if SQLite fails or a stored ID is invalid.
    pub fn latest_completed_scan(&self) -> Result<Option<ScanId>, StoreError> {
        let id: Option<String> = self
            .conn
            .query_row(
                "SELECT id FROM scans WHERE state = 'completed'
                 ORDER BY started_secs DESC, started_nanos DESC, id DESC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .optional()?;
        id.map(|id| id.parse().map_err(|_| corrupt("id")))
            .transpose()
    }

    /// When `scan` finished, if it exists and has finished: the time its
    /// facts were observed.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError::Corrupt`] for an invalid stored time, or
    /// [`StoreError`] if SQLite fails.
    pub fn scan_finished_at(&self, scan: ScanId) -> Result<Option<Timestamp>, StoreError> {
        let row: Option<(Option<i64>, Option<u32>)> = self
            .conn
            .query_row(
                "SELECT finished_secs, finished_nanos FROM scans WHERE id = ?1",
                [scan.to_string()],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        match row {
            Some((secs, nanos)) => join_time("scans.finished", secs, nanos),
            None => Ok(None),
        }
    }

    /// The roots of `scan`, ordered by path bytes.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError::Corrupt`] for an invalid stored path, or
    /// [`StoreError`] if SQLite fails.
    pub fn scan_roots(&self, scan: ScanId) -> Result<Vec<RawPath>, StoreError> {
        let mut query = self.conn.prepare_cached(
            "SELECT flavor, path FROM scan_roots WHERE scan_id = ?1 ORDER BY flavor, path",
        )?;
        let rows = query.query_map([scan.to_string()], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, Vec<u8>>(1)?))
        })?;
        rows.map(|row| {
            let (flavor, path) = row?;
            RawPath::from_stored(parse_flavor(&flavor)?, path)
                .map_err(|_| corrupt("scan_roots.path"))
        })
        .collect()
    }

    /// Scans that started but never finished (still `running`), oldest first.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] if SQLite fails or a stored ID is invalid.
    pub fn unfinished_scans(&self) -> Result<Vec<ScanId>, StoreError> {
        let mut query = self.conn.prepare_cached(
            "SELECT id FROM scans WHERE state = 'running' ORDER BY started_secs, started_nanos, id",
        )?;
        let ids = query.query_map([], |row| row.get::<_, String>(0))?;
        ids.map(|id| id?.parse().map_err(|_| corrupt("id")))
            .collect()
    }

    /// Directories of a scan that were discovered but never listed: where an
    /// interrupted scan resumes, in the scan's [order](Self). Directories on a
    /// different volume than their scan root (mount points the scan
    /// deliberately skipped) are excluded.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] if SQLite fails or a stored value is invalid.
    pub fn frontier(&self, scan: ScanId) -> Result<Vec<PendingDirectory>, StoreError> {
        // Volume of each root, from the root's own stored entry.
        let mut roots = Vec::new();
        for root in self.scan_roots(scan)? {
            if let Some(entry) = self.entry_at(scan, &root)? {
                roots.push((root, entry.size.identity.volume));
            }
        }

        let mut pending_query = self.conn.prepare_cached(
            "SELECT d.flavor, d.path, e.name, e.volume, e.file_id
             FROM dirs d CROSS JOIN entries e ON e.dir = d.id
             WHERE d.scan = (SELECT key FROM scans WHERE id = ?1) AND e.kind = 'directory'
               AND NOT EXISTS (SELECT 1 FROM listings l WHERE l.dir = e.dir AND l.name = e.name)
             ORDER BY d.flavor, d.path, e.name",
        )?;
        let rows = pending_query.query_map([scan.to_string()], |row| {
            Ok((
                path_from_row(row),
                row.get::<_, String>(3)?,
                row.get::<_, Vec<u8>>(4)?,
            ))
        })?;
        let mut pending = Vec::new();
        for row in rows {
            let (path, volume, file_id) = row?;
            let path = path?;
            let volume = VolumeId::new(volume).map_err(|_| corrupt("volume"))?;
            let file = u128::from_be_bytes(file_id.try_into().map_err(|_| corrupt("file_id"))?);
            let Some(root_volume) = root_volume_for(&roots, &path) else {
                continue;
            };
            if volume != root_volume {
                continue; // a mount point the scan chose not to enter
            }
            pending.push(PendingDirectory {
                path,
                identity: FileIdentity {
                    volume,
                    file: FileId::new(file),
                },
                root_volume,
            });
        }
        Ok(pending)
    }

    /// Records the per-root totals of a scan as one snapshot.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] if a value is out of range or SQLite fails; no row
    /// is stored then.
    pub fn record_snapshot(
        &mut self,
        snapshot: SnapshotId,
        scan: ScanId,
        roots: &[RootSnapshot],
    ) -> Result<(), StoreError> {
        let tx = self.conn.transaction()?;
        {
            let mut insert = tx.prepare_cached(
                "INSERT INTO snapshots (id, scan_id, taken_secs, taken_nanos, flavor, root, files, logical, allocated,
                                        reclaimable, exact, complete)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            )?;
            for r in roots {
                insert.execute(params![
                    snapshot.to_string(),
                    scan.to_string(),
                    r.taken_at.unix_seconds(),
                    r.taken_at.subsec_nanos(),
                    flavor_text(r.root.flavor()),
                    r.root.as_bytes(),
                    to_i64("files", r.files)?,
                    to_i64("logical", r.logical.get())?,
                    to_i64("allocated", r.allocated.get())?,
                    to_i64("reclaimable", r.reclaimable.get())?,
                    r.exact,
                    r.complete,
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// The history of one root, oldest first.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError::Corrupt`] for an invalid row, or [`StoreError`] if
    /// SQLite fails.
    pub fn snapshots(&self, root: &RawPath) -> Result<Vec<RootSnapshot>, StoreError> {
        let mut query = self.conn.prepare_cached(
            "SELECT taken_secs, taken_nanos, files, logical, allocated, reclaimable, exact, complete FROM snapshots
             WHERE flavor = ?1 AND root = ?2 ORDER BY taken_secs, taken_nanos",
        )?;
        let rows = query.query_map(
            params![flavor_text(root.flavor()), root.as_bytes()],
            |row| {
                Ok((|| -> Result<RootSnapshot, StoreError> {
                    Ok(RootSnapshot {
                        root: root.clone(),
                        taken_at: join_time("taken", row.get(0)?, row.get(1)?)?
                            .ok_or_else(|| corrupt("taken"))?,
                        files: from_i64("files", row.get(2)?)?,
                        logical: ByteCount::new(from_i64("logical", row.get(3)?)?),
                        allocated: ByteCount::new(from_i64("allocated", row.get(4)?)?),
                        reclaimable: ByteCount::new(from_i64("reclaimable", row.get(5)?)?),
                        exact: row.get(6)?,
                        complete: row.get(7)?,
                    })
                })())
            },
        )?;
        rows.map(|row| row?).collect()
    }

    /// Number of indexed entries, including directories, in this scan only.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] if SQLite fails or the count is invalid.
    pub fn entry_count(&self, scan: ScanId) -> Result<u64, StoreError> {
        let count = self.conn.query_row(
            "SELECT count(*) FROM dirs d CROSS JOIN entries e ON e.dir = d.id
             WHERE d.scan = (SELECT key FROM scans WHERE id = ?1)",
            [scan.to_string()],
            |row| row.get(0),
        )?;
        from_i64("entry_count", count)
    }

    /// All entries of a scan, in the scan's [order](Self).
    ///
    /// # Errors
    ///
    /// Returns [`StoreError::Corrupt`] for any row that fails validation, or
    /// [`StoreError`] if SQLite fails.
    pub fn entries(&self, scan: ScanId) -> Result<Vec<FilesystemEntry>, StoreError> {
        let mut query = self.conn.prepare_cached(select_entries!("", ""))?;
        let rows = query.query_map([scan.to_string()], |row| Ok(entry_from_row(row)))?;
        rows.map(|row| row?).collect()
    }

    /// Up to `limit` entries of a scan that come after `after` in the scan's
    /// [order](Self) (flavor, directory path, name), so a whole scan can be
    /// read a page at a time instead of all at once. Start with `None`;
    /// continue from the last path of the previous page; an empty page is the
    /// end. Each page is an index seek: the rest of `after`'s directory, then
    /// the directories after it.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError::Corrupt`] for any row that fails validation, or
    /// [`StoreError`] if SQLite fails.
    pub fn entries_page(
        &self,
        scan: ScanId,
        after: Option<&RawPath>,
        limit: usize,
    ) -> Result<Vec<FilesystemEntry>, StoreError> {
        let mut page = Vec::new();
        // Every flavor sorts after the empty text, so `None` starts at the first
        // directory.
        let (flavor, dir) = match after {
            Some(after) => {
                let (dir, name) = dirs::split(after);
                let flavor = flavor_text(after.flavor());
                let mut query = self.conn.prepare_cached(select_entries!(
                    "AND d.flavor = ?2 AND d.path = ?3 AND e.name > ?4",
                    "LIMIT ?5"
                ))?;
                let rows = query.query_map(
                    params![scan.to_string(), flavor, dir, name, sql_limit(limit)?],
                    |row| Ok(entry_from_row(row)),
                )?;
                for row in rows {
                    page.push(row??);
                }
                (flavor, dir)
            }
            None => ("", &[][..]),
        };
        let rest = limit.saturating_sub(page.len());
        if rest > 0 {
            let mut query = self.conn.prepare_cached(select_entries!(
                "AND (d.flavor, d.path) > (?2, ?3)",
                "LIMIT ?4"
            ))?;
            let rows = query.query_map(
                params![scan.to_string(), flavor, dir, sql_limit(rest)?],
                |row| Ok(entry_from_row(row)),
            )?;
            for row in rows {
                page.push(row??);
            }
        }
        Ok(page)
    }

    /// The entry stored for exactly `path`, if any.
    fn entry_at(
        &self,
        scan: ScanId,
        path: &RawPath,
    ) -> Result<Option<FilesystemEntry>, StoreError> {
        let (dir, name) = dirs::split(path);
        let mut query = self.conn.prepare_cached(select_entries!(
            "AND d.flavor = ?2 AND d.path = ?3 AND e.name = ?4",
            ""
        ))?;
        query
            .query_row(
                params![scan.to_string(), flavor_text(path.flavor()), dir, name],
                |row| Ok(entry_from_row(row)),
            )
            .optional()?
            .transpose()
    }

    /// The entries of a scan at and below `dir` (whole segments: `/a/b` and
    /// `/a/b/c`, never `/a/bc`), in the scan's [order](Self). Index seeks
    /// only: `dir`'s own entry, then a range of directory paths, so only that
    /// part of the scan is read.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError::Corrupt`] for any row that fails validation, or
    /// [`StoreError`] if SQLite fails.
    pub fn entries_within(
        &self,
        scan: ScanId,
        dir: &RawPath,
    ) -> Result<Vec<FilesystemEntry>, StoreError> {
        let (start, end) = dirs::below_range(dir);
        let mut within = Vec::new();
        // `dir`'s own entry, unless the range holds it (`/` is in `/`).
        if !dirs::split(dir).0.starts_with(&start) {
            within.extend(self.entry_at(scan, dir)?);
        }
        let mut query = self.conn.prepare_cached(select_entries!(
            "AND d.flavor = ?2 AND d.path >= ?3 AND d.path < ?4",
            ""
        ))?;
        let rows = query.query_map(
            params![scan.to_string(), flavor_text(dir.flavor()), start, end],
            |row| Ok(entry_from_row(row)),
        )?;
        for row in rows {
            within.push(row??);
        }
        Ok(within)
    }

    /// The listing outcomes of a scan at and below `dir`, as for
    /// [`Self::entries_within`].
    ///
    /// # Errors
    ///
    /// Returns [`StoreError::Corrupt`] for any row that fails validation, or
    /// [`StoreError`] if SQLite fails.
    pub fn listings_within(
        &self,
        scan: ScanId,
        dir: &RawPath,
    ) -> Result<Vec<(RawPath, DirOutcome)>, StoreError> {
        let (start, end) = dirs::below_range(dir);
        let mut within = Vec::new();
        let (own_dir, own_name) = dirs::split(dir);
        let flavor = flavor_text(dir.flavor());
        if !own_dir.starts_with(&start) {
            let mut query = self.conn.prepare_cached(select_listings!(
                "AND d.flavor = ?2 AND d.path = ?3 AND l.name = ?4"
            ))?;
            let own = query
                .query_row(
                    params![scan.to_string(), flavor, own_dir, own_name],
                    |row| Ok(listing_from_row(row)),
                )
                .optional()?;
            within.extend(own.transpose()?);
        }
        let mut query = self.conn.prepare_cached(select_listings!(
            "AND d.flavor = ?2 AND d.path >= ?3 AND d.path < ?4"
        ))?;
        let rows = query.query_map(params![scan.to_string(), flavor, start, end], |row| {
            Ok(listing_from_row(row))
        })?;
        for row in rows {
            within.push(row??);
        }
        Ok(within)
    }

    /// Every directory listing outcome of a scan, in the scan's [order](Self)
    /// (roadmap 10.7): how completely each directory was read.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError::Corrupt`] for any row that fails validation, or
    /// [`StoreError`] if SQLite fails.
    pub fn listings(&self, scan: ScanId) -> Result<Vec<(RawPath, DirOutcome)>, StoreError> {
        let mut query = self.conn.prepare_cached(select_listings!(""))?;
        let rows = query.query_map([scan.to_string()], |row| Ok(listing_from_row(row)))?;
        rows.map(|row| row?).collect()
    }
}

/// A `LIMIT` value.
fn sql_limit(limit: usize) -> Result<i64, StoreError> {
    i64::try_from(limit).map_err(|_| corrupt("limit"))
}

/// The path of a row whose first three columns are flavor, directory path and
/// name.
fn path_from_row(row: &Row<'_>) -> Result<RawPath, StoreError> {
    let flavor = parse_flavor(&row.get::<_, String>(0)?)?;
    let mut path: Vec<u8> = row.get(1)?;
    path.extend_from_slice(&row.get::<_, Vec<u8>>(2)?);
    RawPath::from_stored(flavor, path).map_err(|_| corrupt("path"))
}

/// A listing row: flavor, directory path, name, access, complete.
fn listing_from_row(row: &Row<'_>) -> Result<(RawPath, DirOutcome), StoreError> {
    Ok((
        path_from_row(row)?,
        DirOutcome {
            access: parse_access(&row.get::<_, String>(3)?)?,
            complete: row.get(4)?,
        },
    ))
}

/// The smallest byte string greater than every string that starts with
/// `prefix`, or `None` if there is none (the prefix is all `0xFF`).
fn prefix_end(prefix: &[u8]) -> Option<Vec<u8>> {
    let mut end = prefix.to_vec();
    while let Some(last) = end.pop() {
        if last < u8::MAX {
            end.push(last + 1);
            return Some(end);
        }
    }
    None
}

/// Volume of the most specific root containing `path` (whole segments).
fn root_volume_for(roots: &[(RawPath, VolumeId)], path: &RawPath) -> Option<VolumeId> {
    roots
        .iter()
        .filter(|(root, _)| {
            root.flavor() == path.flavor()
                && path
                    .as_bytes()
                    .strip_prefix(root.as_bytes())
                    .is_some_and(|rest| {
                        rest.is_empty()
                            || root.as_bytes().ends_with(b"/")
                            || root.as_bytes().ends_with(b"\\")
                            || rest.starts_with(b"/")
                            || rest.starts_with(b"\\")
                    })
        })
        .max_by_key(|(root, _)| root.as_bytes().len())
        .map(|(_, volume)| volume.clone())
}

fn corrupt(column: &'static str) -> StoreError {
    StoreError::Corrupt { column }
}

fn to_i64(column: &'static str, value: u64) -> Result<i64, StoreError> {
    i64::try_from(value).map_err(|_| StoreError::OutOfRange { column })
}

fn from_i64(column: &'static str, value: i64) -> Result<u64, StoreError> {
    u64::try_from(value).map_err(|_| corrupt(column))
}

fn split(time: Option<Timestamp>) -> (Option<i64>, Option<u32>) {
    time.map_or((None, None), |t| {
        (Some(t.unix_seconds()), Some(t.subsec_nanos()))
    })
}

fn join_time(
    column: &'static str,
    secs: Option<i64>,
    nanos: Option<u32>,
) -> Result<Option<Timestamp>, StoreError> {
    match (secs, nanos) {
        (None, None) => Ok(None),
        (Some(s), Some(n)) => Timestamp::from_unix(s, n)
            .map(Some)
            .map_err(|_| corrupt(column)),
        _ => Err(corrupt(column)),
    }
}

const fn flavor_text(flavor: PathFlavor) -> &'static str {
    match flavor {
        PathFlavor::Unix => "unix",
        PathFlavor::Windows => "windows",
    }
}

fn parse_flavor(text: &str) -> Result<PathFlavor, StoreError> {
    match text {
        "unix" => Ok(PathFlavor::Unix),
        "windows" => Ok(PathFlavor::Windows),
        _ => Err(corrupt("flavor")),
    }
}

const fn kind_text(kind: EntryKind) -> &'static str {
    match kind {
        EntryKind::File => "file",
        EntryKind::Directory => "directory",
        EntryKind::Symlink => "symlink",
        EntryKind::Junction => "junction",
        EntryKind::OtherReparsePoint => "other_reparse_point",
        EntryKind::Special => "special",
    }
}

fn parse_kind(text: &str) -> Result<EntryKind, StoreError> {
    Ok(match text {
        "file" => EntryKind::File,
        "directory" => EntryKind::Directory,
        "symlink" => EntryKind::Symlink,
        "junction" => EntryKind::Junction,
        "other_reparse_point" => EntryKind::OtherReparsePoint,
        "special" => EntryKind::Special,
        _ => return Err(corrupt("kind")),
    })
}

const fn scan_state_text(state: ScanState) -> &'static str {
    match state {
        ScanState::Created => "created",
        ScanState::Running => "running",
        ScanState::Paused => "paused",
        ScanState::Completed => "completed",
        ScanState::Cancelled => "cancelled",
        ScanState::Failed => "failed",
    }
}

fn parse_scan_state(text: &str) -> Result<ScanState, StoreError> {
    Ok(match text {
        "created" => ScanState::Created,
        "running" => ScanState::Running,
        "paused" => ScanState::Paused,
        "completed" => ScanState::Completed,
        "cancelled" => ScanState::Cancelled,
        "failed" => ScanState::Failed,
        _ => return Err(corrupt("state")),
    })
}

// Size-flag bits (schema v1).
const SPARSE: i64 = 1;
const COMPRESSED: i64 = 2;
const DATALESS: i64 = 4;
const CLOUD_PLACEHOLDER: i64 = 8;
const MAY_SHARE_BLOCKS: i64 = 16;

fn size_flag_bits(f: SizeFlags) -> i64 {
    [
        (f.sparse, SPARSE),
        (f.compressed, COMPRESSED),
        (f.dataless, DATALESS),
        (f.cloud_placeholder, CLOUD_PLACEHOLDER),
        (f.may_share_blocks, MAY_SHARE_BLOCKS),
    ]
    .into_iter()
    .filter(|(set, _)| *set)
    .fold(0, |acc, (_, bit)| acc | bit)
}

const fn size_flags_from(bits: i64) -> SizeFlags {
    SizeFlags {
        sparse: bits & SPARSE != 0,
        compressed: bits & COMPRESSED != 0,
        dataless: bits & DATALESS != 0,
        cloud_placeholder: bits & CLOUD_PLACEHOLDER != 0,
        may_share_blocks: bits & MAY_SHARE_BLOCKS != 0,
    }
}

// Protection bits (schema v1).
const SYSTEM_RESTRICTED: i64 = 1;
const UNDELETABLE: i64 = 2;
const SYSTEM_ATTRIBUTE: i64 = 4;
const HIDDEN: i64 = 8;

fn protection_bits(p: Protection) -> i64 {
    [
        (p.system_restricted, SYSTEM_RESTRICTED),
        (p.undeletable, UNDELETABLE),
        (p.system_attribute, SYSTEM_ATTRIBUTE),
        (p.hidden, HIDDEN),
    ]
    .into_iter()
    .filter(|(set, _)| *set)
    .fold(0, |acc, (_, bit)| acc | bit)
}

const fn protection_from(bits: i64) -> Protection {
    Protection {
        system_restricted: bits & SYSTEM_RESTRICTED != 0,
        undeletable: bits & UNDELETABLE != 0,
        system_attribute: bits & SYSTEM_ATTRIBUTE != 0,
        hidden: bits & HIDDEN != 0,
    }
}

/// Access states use the domain's serde form, so storage shares one validated
/// vocabulary with every other boundary.
fn access_text(access: AccessState) -> Result<String, StoreError> {
    serde_json::to_string(&access).map_err(|_| corrupt("access"))
}

fn parse_access(text: &str) -> Result<AccessState, StoreError> {
    serde_json::from_str(text).map_err(|_| corrupt("access"))
}

/// An entry row: flavor, directory path and name, then the entry's columns.
fn entry_from_row(row: &Row<'_>) -> Result<FilesystemEntry, StoreError> {
    let path = path_from_row(row)?;
    let volume = VolumeId::new(row.get::<_, String>(3)?).map_err(|_| corrupt("volume"))?;
    let file_bytes: Vec<u8> = row.get(4)?;
    let file = u128::from_be_bytes(file_bytes.try_into().map_err(|_| corrupt("file_id"))?);
    let clone_id = row
        .get::<_, Option<Vec<u8>>>(9)?
        .map(|b| {
            <[u8; 8]>::try_from(b)
                .map(|b| CloneId::new(u64::from_be_bytes(b)))
                .map_err(|_| corrupt("clone_id"))
        })
        .transpose()?;
    let link_count = u32::try_from(row.get::<_, i64>(10)?)
        .ok()
        .and_then(NonZeroU32::new)
        .ok_or_else(|| corrupt("link_count"))?;
    Ok(FilesystemEntry {
        path,
        kind: parse_kind(&row.get::<_, String>(5)?)?,
        size: SizeFacts {
            identity: FileIdentity {
                volume,
                file: FileId::new(file),
            },
            logical: ByteCount::new(from_i64("logical", row.get(6)?)?),
            allocated: ByteCount::new(from_i64("allocated", row.get(7)?)?),
            private: row
                .get::<_, Option<i64>>(8)?
                .map(|p| from_i64("private", p).map(ByteCount::new))
                .transpose()?,
            clone_id,
            link_count,
            flags: size_flags_from(row.get(11)?),
        },
        times: EntryTimes {
            modified: join_time("modified", row.get(13)?, row.get(14)?)?,
            accessed: join_time("accessed", row.get(15)?, row.get(16)?)?,
            changed: join_time("changed", row.get(17)?, row.get(18)?)?,
            created: join_time("created", row.get(19)?, row.get(20)?)?,
        },
        protection: protection_from(row.get(12)?),
        access: parse_access(&row.get::<_, String>(21)?)?,
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use lumen_domain::{DenialReason, FailureKind};
    use lumen_testkit::Fixture;
    use proptest::prelude::*;

    use super::*;
    use crate::open_index;

    pub(crate) fn store() -> Result<(Fixture, IndexStore), Box<dyn std::error::Error>> {
        let fx = Fixture::build(&[])?;
        let conn = open_index(&fx.path("index.db")?)?;
        Ok((fx, IndexStore::new(conn)))
    }

    pub(crate) fn scan_id(n: u8) -> ScanId {
        format!("0190f1c2-7a3b-7c4d-8e5f-6a7b8c9d0e{n:02x}")
            .parse()
            .unwrap_or_else(|_| unreachable!())
    }

    pub(crate) fn entry(path: &[u8], file: u128) -> FilesystemEntry {
        FilesystemEntry {
            path: RawPath::from_unix_bytes(path.to_vec()).unwrap_or_else(|_| unreachable!()),
            kind: EntryKind::File,
            size: SizeFacts {
                identity: FileIdentity {
                    volume: VolumeId::new("dev:1").unwrap_or_else(|_| unreachable!()),
                    file: FileId::new(file),
                },
                logical: ByteCount::new(10),
                allocated: ByteCount::new(4096),
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

    /// The scan's order: flavor, directory path, name.
    fn order(path: &RawPath) -> (&'static str, &[u8], &[u8]) {
        let (dir, name) = dirs::split(path);
        (flavor_text(path.flavor()), dir, name)
    }

    #[test]
    fn entries_round_trip_including_extremes() -> Result<(), Box<dyn std::error::Error>> {
        let (_fx, mut store) = store()?;
        let scan = scan_id(1);
        let root = RawPath::from_unix_bytes(b"/r".to_vec())?;
        store.begin_scan(scan, Timestamp::UNIX_EPOCH, std::slice::from_ref(&root))?;

        let mut rich = entry(b"/r/\xff\nodd", u128::MAX);
        rich.size.logical = ByteCount::new(i64::MAX.unsigned_abs());
        rich.size.private = Some(ByteCount::new(0));
        rich.size.clone_id = Some(CloneId::new(u64::MAX));
        rich.size.link_count = NonZeroU32::new(u32::MAX).unwrap_or(NonZeroU32::MIN);
        rich.size.flags = SizeFlags {
            sparse: true,
            compressed: true,
            dataless: true,
            cloud_placeholder: true,
            may_share_blocks: true,
        };
        rich.protection = Protection {
            system_restricted: true,
            undeletable: true,
            system_attribute: true,
            hidden: true,
        };
        rich.times = EntryTimes {
            modified: Some(Timestamp::MIN),
            accessed: Some(Timestamp::MAX),
            changed: Some("1969-12-31T23:59:59.5Z".parse()?),
            created: None,
        };
        rich.access = AccessState::Denied(DenialReason::Tcc);
        let windows = FilesystemEntry {
            path: RawPath::from_windows_wide(&[u16::from(b'C'), u16::from(b':'), 0xD800])?,
            kind: EntryKind::Junction,
            access: AccessState::Failed(FailureKind::Vanished),
            ..entry(b"/unused", 7)
        };
        let plain = entry(b"/r/a", 1);
        store.insert_entries(scan, &[rich.clone(), windows.clone(), plain.clone()])?;

        let mut expected = vec![plain, rich, windows];
        expected.sort_by(|a, b| order(&a.path).cmp(&order(&b.path)));
        assert_eq!(store.entries(scan)?, expected);
        Ok(())
    }

    #[test]
    fn batches_are_all_or_nothing() -> Result<(), Box<dyn std::error::Error>> {
        let (_fx, mut store) = store()?;
        let scan = scan_id(2);
        store.begin_scan(scan, Timestamp::UNIX_EPOCH, &[])?;
        let mut too_big = entry(b"/r/huge", 2);
        too_big.size.allocated = ByteCount::new(u64::MAX);
        let result = store.insert_entries(scan, &[entry(b"/r/ok", 1), too_big]);
        assert!(matches!(
            result,
            Err(StoreError::OutOfRange {
                column: "allocated"
            })
        ));
        assert_eq!(
            store.entries(scan)?,
            vec![],
            "the valid first row was rolled back too"
        );
        Ok(())
    }

    #[test]
    fn tampered_rows_are_reported_not_misread() -> Result<(), Box<dyn std::error::Error>> {
        for (column, tamper) in [
            ("volume", "UPDATE entries SET volume = 'has space'"),
            (
                "access",
                "UPDATE entries SET access = '{\"state\":\"empty\"}'",
            ),
            ("modified", "UPDATE entries SET modified_secs = 0"),
            ("logical", "UPDATE entries SET logical = -1"),
            ("path", "UPDATE entries SET name = x'00'"),
            ("path", "UPDATE dirs SET flavor = 'windows', path = x'ff'"),
            ("flavor", "UPDATE dirs SET flavor = 'dos'"),
        ] {
            let (_fx, mut store) = store()?;
            let scan = scan_id(3);
            store.begin_scan(scan, Timestamp::UNIX_EPOCH, &[])?;
            store.insert_entries(scan, &[entry(b"/r/a", 1)])?;
            // CHECK constraints stop some tampering at write time; anything that
            // gets through must still fail validation on read.
            if store.conn.execute(tamper, []).is_ok() {
                assert!(
                    matches!(store.entries(scan), Err(StoreError::Corrupt { column: c }) if c == column),
                    "{column}"
                );
            }
        }
        Ok(())
    }

    #[test]
    fn the_latest_completed_scan_and_its_roots_are_found() -> Result<(), Box<dyn std::error::Error>>
    {
        let (_fx, mut store) = store()?;
        assert_eq!(store.latest_completed_scan()?, None);
        let root = RawPath::from_unix_bytes(b"/Users/ana".to_vec())?;
        for (n, secs, state) in [
            (1, 10, ScanState::Completed),
            (2, 20, ScanState::Completed),
            (3, 30, ScanState::Cancelled),
        ] {
            store.begin_scan(
                scan_id(n),
                Timestamp::from_unix(secs, 0)?,
                std::slice::from_ref(&root),
            )?;
            store.finish_scan(scan_id(n), state, Timestamp::from_unix(secs + 1, 0)?)?;
        }
        store.begin_scan(scan_id(4), Timestamp::from_unix(40, 0)?, &[])?;
        assert_eq!(
            store.latest_completed_scan()?,
            Some(scan_id(2)),
            "newer cancelled and running scans are never assessed"
        );
        assert_eq!(store.scan_roots(scan_id(2))?, [root]);
        assert_eq!(store.scan_roots(scan_id(4))?, []);
        assert_eq!(
            store.scan_finished_at(scan_id(2))?,
            Some(Timestamp::from_unix(21, 0)?)
        );
        assert_eq!(store.scan_finished_at(scan_id(4))?, None, "still running");
        assert_eq!(store.scan_finished_at(scan_id(9))?, None, "no such scan");
        Ok(())
    }

    #[test]
    fn scan_lifecycle_and_listings() -> Result<(), Box<dyn std::error::Error>> {
        let (_fx, mut store) = store()?;
        let scan = scan_id(4);
        assert!(matches!(
            store.finish_scan(scan, ScanState::Completed, Timestamp::UNIX_EPOCH),
            Err(StoreError::UnknownScan { .. })
        ));
        store.begin_scan(scan, Timestamp::UNIX_EPOCH, &[])?;
        assert_eq!(store.scan_state(scan)?, Some(ScanState::Running));
        let dir = RawPath::from_unix_bytes(b"/r".to_vec())?;
        store.insert_listings(
            scan,
            &[(
                dir,
                DirOutcome {
                    access: AccessState::Denied(DenialReason::Posix),
                    complete: false,
                },
            )],
        )?;
        assert_eq!(
            store.listings(scan)?,
            [(
                RawPath::from_unix_bytes(b"/r".to_vec())?,
                DirOutcome {
                    access: AccessState::Denied(DenialReason::Posix),
                    complete: false,
                },
            )]
        );
        store.finish_scan(scan, ScanState::Cancelled, "2026-10-12T00:00:00Z".parse()?)?;
        assert_eq!(store.scan_state(scan)?, Some(ScanState::Cancelled));
        assert_eq!(store.scan_state(scan_id(5))?, None);
        Ok(())
    }

    fn dir(path: &[u8], file: u128, volume: &str) -> FilesystemEntry {
        let mut e = entry(path, file);
        e.kind = EntryKind::Directory;
        e.access = AccessState::NotScanned;
        e.size.identity.volume = VolumeId::new(volume).unwrap_or_else(|_| unreachable!());
        e
    }

    fn raw(p: &[u8]) -> RawPath {
        RawPath::from_unix_bytes(p.to_vec()).unwrap_or_else(|_| unreachable!())
    }

    /// `EXPLAIN QUERY PLAN` of `sql` with `params`, one step per line.
    fn plan(
        store: &IndexStore,
        sql: &str,
        params: impl rusqlite::Params,
    ) -> Result<String, Box<dyn std::error::Error>> {
        Ok(store
            .conn
            .prepare(&format!("EXPLAIN QUERY PLAN {sql}"))?
            .query_map(params, |row| row.get::<_, String>(3))?
            .collect::<Result<Vec<_>, _>>()?
            .join("\n"))
    }

    #[test]
    fn a_scan_reads_back_page_by_page_and_by_subtree() -> Result<(), Box<dyn std::error::Error>> {
        let (_fx, mut store) = store()?;
        let scan = scan_id(9);
        store.begin_scan(scan, Timestamp::UNIX_EPOCH, &[])?;
        assert_eq!(store.entry_count(scan)?, 0);
        let paths: [&[u8]; 10] = [
            b"/",
            b"/a",
            b"/a/b",
            b"/a/b/c",
            b"/a/b/d",
            b"/a/bc",
            b"/a/c",
            b"/a/c/x/y",
            b"/z",
            b"rel",
        ];
        let entries: Vec<FilesystemEntry> =
            paths.iter().zip(1..).map(|(p, n)| entry(p, n)).collect();
        store.insert_entries(scan, &entries)?;
        // Another scan's rows never show.
        store.begin_scan(scan_id(10), Timestamp::UNIX_EPOCH, &[])?;
        store.insert_entries(scan_id(10), &[entry(b"/a/b/c", 99), entry(b"/a/b/e", 98)])?;
        let all = store.entries(scan)?;
        assert_eq!(store.entry_count(scan)?, 10);
        assert_eq!(store.entry_count(scan_id(10))?, 2);
        let ordered: Vec<&[u8]> = all.iter().map(|e| e.path.as_bytes()).collect();
        assert_eq!(
            ordered,
            [
                &b"rel"[..],
                b"/",
                b"/a",
                b"/z",
                b"/a/b",
                b"/a/bc",
                b"/a/c",
                b"/a/b/c",
                b"/a/b/d",
                b"/a/c/x/y"
            ],
            "by directory path, then name"
        );
        for size in 1..=all.len() + 1 {
            let mut paged = Vec::new();
            let mut after: Option<RawPath> = None;
            loop {
                let page = store.entries_page(scan, after.as_ref(), size)?;
                assert!(page.len() <= size);
                let Some(last) = page.last() else { break };
                after = Some(last.path.clone());
                paged.extend(page);
            }
            assert_eq!(paged, all, "pages of {size} cover the scan once, in order");
        }

        let within = |dir: &[u8]| -> Result<Vec<Vec<u8>>, StoreError> {
            Ok(store
                .entries_within(scan, &raw(dir))?
                .iter()
                .map(|e| e.path.as_bytes().to_vec())
                .collect())
        };
        assert_eq!(
            within(b"/a/b")?,
            [&b"/a/b"[..], b"/a/b/c", b"/a/b/d"],
            "whole segments"
        );
        assert_eq!(within(b"/a/b/")?, [&b"/a/b/c"[..], b"/a/b/d"]);
        assert_eq!(within(b"/a/c")?, [&b"/a/c"[..], b"/a/c/x/y"]);
        assert_eq!(within(b"/a/b/c")?, [b"/a/b/c"]);
        assert_eq!(within(b"/nowhere")?, Vec::<Vec<u8>>::new());
        assert_eq!(within(b"/")?.len(), 9, "the root and everything absolute");
        assert_eq!(prefix_end(b"ab"), Some(b"ac".to_vec()));
        assert_eq!(prefix_end(&[b'a', 0xFF]), Some(b"b".to_vec()));
        assert_eq!(prefix_end(&[0xFF, 0xFF]), None);
        Ok(())
    }

    #[test]
    fn listings_read_back_by_subtree() -> Result<(), Box<dyn std::error::Error>> {
        let (_fx, mut store) = store()?;
        let scan = scan_id(9);
        store.begin_scan(scan, Timestamp::UNIX_EPOCH, &[])?;
        let readable = DirOutcome {
            access: AccessState::Readable,
            complete: true,
        };
        let listed: Vec<(RawPath, DirOutcome)> = [&b"/a"[..], b"/a/b", b"/a/bc", b"/z"]
            .into_iter()
            .map(|p| (raw(p), readable))
            .collect();
        store.insert_listings(scan, &listed)?;
        let listed_within: Vec<RawPath> = store
            .listings_within(scan, &raw(b"/a/b"))?
            .into_iter()
            .map(|(p, _)| p)
            .collect();
        assert_eq!(listed_within, [raw(b"/a/b")]);
        let listed_within: Vec<RawPath> = store
            .listings_within(scan, &raw(b"/a"))?
            .into_iter()
            .map(|(p, _)| p)
            .collect();
        assert_eq!(listed_within, [raw(b"/a"), raw(b"/a/b"), raw(b"/a/bc")]);
        assert_eq!(store.listings(scan)?.len(), 4);
        Ok(())
    }

    #[test]
    fn pages_and_subtrees_are_index_seeks_not_sorts() -> Result<(), Box<dyn std::error::Error>> {
        let (_fx, store) = store()?;
        let seeks = |sql: &str, plan: &str, dir_seek: &str, inner: &str| {
            assert!(plan.contains(dir_seek), "{sql}:\n{plan}");
            assert!(plan.contains(inner), "{sql}:\n{plan}");
            assert!(!plan.contains("TEMP B-TREE"), "{sql}:\n{plan}");
            assert!(!plan.contains("SCAN "), "{sql}:\n{plan}");
        };
        // The rest of a page's last directory, then the directories after it.
        let sql = select_entries!(
            "AND d.flavor = ?2 AND d.path = ?3 AND e.name > ?4",
            "LIMIT ?5"
        );
        let p = plan(
            &store,
            sql,
            params!["s", "unix", b"/a/".as_slice(), b"b".as_slice(), 10],
        )?;
        seeks(
            sql,
            &p,
            "(scan=? AND flavor=? AND path=?)",
            "SEARCH e USING PRIMARY KEY (dir=? AND name>?)",
        );
        let sql = select_entries!("AND (d.flavor, d.path) > (?2, ?3)", "LIMIT ?4");
        let p = plan(&store, sql, params!["s", "unix", b"/a/".as_slice(), 10])?;
        seeks(
            sql,
            &p,
            "(scan=? AND (flavor,path)>(?,?))",
            "SEARCH e USING PRIMARY KEY (dir=?)",
        );
        // A subtree: its own entry, then a range of directory paths.
        let sql = select_entries!("AND d.flavor = ?2 AND d.path = ?3 AND e.name = ?4", "");
        let p = plan(
            &store,
            sql,
            params!["s", "unix", b"/a/".as_slice(), b"b".as_slice()],
        )?;
        seeks(
            sql,
            &p,
            "(scan=? AND flavor=? AND path=?)",
            "SEARCH e USING PRIMARY KEY (dir=? AND name=?)",
        );
        let sql = select_entries!("AND d.flavor = ?2 AND d.path >= ?3 AND d.path < ?4", "");
        let p = plan(
            &store,
            sql,
            params!["s", "unix", b"/a/b/".as_slice(), b"/a/b0".as_slice()],
        )?;
        seeks(
            sql,
            &p,
            "(scan=? AND flavor=? AND path>? AND path<?)",
            "SEARCH e USING PRIMARY KEY (dir=?)",
        );
        let sql = select_listings!("AND d.flavor = ?2 AND d.path >= ?3 AND d.path < ?4");
        let p = plan(
            &store,
            sql,
            params!["s", "unix", b"/a/b/".as_slice(), b"/a/b0".as_slice()],
        )?;
        seeks(
            sql,
            &p,
            "(scan=? AND flavor=? AND path>? AND path<?)",
            "SEARCH l USING PRIMARY KEY (dir=?)",
        );
        Ok(())
    }

    #[test]
    fn re_inserting_a_path_keeps_the_newer_observation() -> Result<(), Box<dyn std::error::Error>> {
        let (_fx, mut store) = store()?;
        let scan = scan_id(7);
        store.begin_scan(scan, Timestamp::UNIX_EPOCH, &[])?;
        store.insert_entries(scan, &[entry(b"/r/a", 1)])?;
        let mut newer = entry(b"/r/a", 1);
        newer.size.logical = ByteCount::new(999);
        store.insert_entries(scan, std::slice::from_ref(&newer))?;
        assert_eq!(store.entries(scan)?, vec![newer]);
        Ok(())
    }

    #[test]
    fn frontier_is_unlisted_directories_on_the_root_volume()
    -> Result<(), Box<dyn std::error::Error>> {
        let (_fx, mut store) = store()?;
        let scan = scan_id(8);
        store.begin_scan(scan, Timestamp::UNIX_EPOCH, &[raw(b"/r")])?;
        store.insert_entries(
            scan,
            &[
                dir(b"/r", 1, "dev:1"),
                dir(b"/r/a", 2, "dev:1"),
                dir(b"/r/b", 3, "dev:1"),
                dir(b"/r/mnt", 4, "dev:2"),
                entry(b"/r/a/file", 5),
            ],
        )?;
        let readable = DirOutcome {
            access: AccessState::Readable,
            complete: true,
        };
        // Nothing listed yet: the root itself is the frontier's first entry.
        let paths = |pending: Vec<PendingDirectory>| {
            pending.into_iter().map(|p| p.path).collect::<Vec<_>>()
        };
        assert_eq!(
            paths(store.frontier(scan)?),
            vec![raw(b"/r"), raw(b"/r/a"), raw(b"/r/b")]
        );
        store.insert_listings(scan, &[(raw(b"/r"), readable), (raw(b"/r/b"), readable)])?;
        let pending = store.frontier(scan)?;
        assert_eq!(
            paths(pending.clone()),
            vec![raw(b"/r/a")],
            "listed dirs, files and mount points excluded"
        );
        assert_eq!(pending[0].identity.file, FileId::new(2));
        assert_eq!(pending[0].root_volume.as_str(), "dev:1");
        Ok(())
    }

    #[test]
    fn unfinished_scans_are_found() -> Result<(), Box<dyn std::error::Error>> {
        let (_fx, mut store) = store()?;
        store.begin_scan(scan_id(9), Timestamp::UNIX_EPOCH, &[])?;
        store.begin_scan(scan_id(10), "2026-01-01T00:00:00Z".parse()?, &[])?;
        store.finish_scan(scan_id(9), ScanState::Completed, Timestamp::UNIX_EPOCH)?;
        assert_eq!(store.unfinished_scans()?, vec![scan_id(10)]);
        Ok(())
    }

    #[test]
    fn snapshots_round_trip_in_time_order() -> Result<(), Box<dyn std::error::Error>> {
        let (_fx, mut store) = store()?;
        let root = raw(b"/Users/ana");
        let other = raw(b"/Volumes/data");
        let snap = |at: &str, files: u64| -> Result<RootSnapshot, Box<dyn std::error::Error>> {
            Ok(RootSnapshot {
                root: root.clone(),
                taken_at: at.parse()?,
                files,
                logical: ByteCount::new(files * 10),
                allocated: ByteCount::new(files * 4096),
                reclaimable: ByteCount::new(files),
                exact: files.is_multiple_of(2),
                complete: true,
            })
        };
        let later = snap("2026-10-12T00:00:00Z", 20)?;
        let earlier = snap("2026-10-05T00:00:00Z", 10)?;
        let elsewhere = RootSnapshot {
            root: other.clone(),
            complete: false,
            ..snap("2026-10-06T00:00:00Z", 3)?
        };
        for (n, snapshots) in [
            (11u8, vec![later.clone(), elsewhere.clone()]),
            (12, vec![earlier.clone()]),
        ] {
            store.begin_scan(scan_id(n), Timestamp::UNIX_EPOCH, &[])?;
            let snapshot: SnapshotId =
                format!("0190f1c2-7a3b-7c4d-8e5f-6a7b8c9d0f{n:02x}").parse()?;
            store.record_snapshot(snapshot, scan_id(n), &snapshots)?;
        }
        assert_eq!(store.snapshots(&root)?, vec![earlier, later]);
        assert_eq!(store.snapshots(&other)?, vec![elsewhere]);
        assert_eq!(store.snapshots(&raw(b"/nowhere"))?, vec![]);
        Ok(())
    }

    proptest! {
        #[test]
        fn arbitrary_entries_round_trip(
            name in prop::collection::vec(1u8.., 1..20),
            file in any::<u128>(),
            logical in 0u64..=i64::MAX.unsigned_abs(),
            flags in 0i64..32,
            protection in 0i64..16,
            secs in Timestamp::MIN.unix_seconds()..=Timestamp::MAX.unix_seconds(),
            nanos in 0u32..1_000_000_000,
        ) {
            let (_fx, mut store) = store().map_err(|e| TestCaseError::fail(e.to_string()))?;
            let scan = scan_id(6);
            let mut path = b"/r/".to_vec();
            path.extend(name.iter().map(|b| if *b == b'/' { b'_' } else { *b }));
            let mut e = entry(&path, file);
            e.size.logical = ByteCount::new(logical);
            e.size.flags = size_flags_from(flags);
            e.protection = protection_from(protection);
            e.times.modified = Some(Timestamp::from_unix(secs, nanos).map_err(|e| TestCaseError::fail(e.to_string()))?);
            store.begin_scan(scan, Timestamp::UNIX_EPOCH, &[]).map_err(|e| TestCaseError::fail(e.to_string()))?;
            store.insert_entries(scan, std::slice::from_ref(&e)).map_err(|e| TestCaseError::fail(e.to_string()))?;
            prop_assert_eq!(store.entries(scan).map_err(|e| TestCaseError::fail(e.to_string()))?, vec![e]);
        }
    }
}
