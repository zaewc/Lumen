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
#[derive(Debug)]
pub struct IndexStore {
    conn: Connection,
}

mod graph;

impl IndexStore {
    /// Wraps a connection opened with [`open_index`](crate::open_index).
    pub fn new(conn: Connection) -> Self {
        Self { conn }
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
            "INSERT INTO scans (id, started_secs, started_nanos, state) VALUES (?1, ?2, ?3, 'running')",
            params![scan.to_string(), started_at.unix_seconds(), started_at.subsec_nanos()],
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
    /// Returns [`StoreError`] if a value is out of SQLite's range or SQLite
    /// fails; no entry of the batch is stored then.
    pub fn insert_entries(
        &mut self,
        scan: ScanId,
        entries: &[FilesystemEntry],
    ) -> Result<(), StoreError> {
        let tx = self.conn.transaction()?;
        {
            let mut insert = tx.prepare_cached(
                "INSERT OR REPLACE INTO entries (scan_id, flavor, path, volume, file_id, kind, logical, allocated, private, clone_id,
                                      link_count, size_flags, protection, modified_secs, modified_nanos, accessed_secs,
                                      accessed_nanos, changed_secs, changed_nanos, created_secs, created_nanos, access)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22)",
            )?;
            for entry in entries {
                let size = &entry.size;
                let (modified, accessed, changed, created) = (
                    split(entry.times.modified),
                    split(entry.times.accessed),
                    split(entry.times.changed),
                    split(entry.times.created),
                );
                insert.execute(params![
                    scan.to_string(),
                    flavor_text(entry.path.flavor()),
                    entry.path.as_bytes(),
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
    /// Returns [`StoreError`] if SQLite fails; no row of the batch is stored
    /// then.
    pub fn insert_listings(
        &mut self,
        scan: ScanId,
        listings: &[(RawPath, DirOutcome)],
    ) -> Result<(), StoreError> {
        let tx = self.conn.transaction()?;
        {
            let mut insert = tx
                .prepare_cached("INSERT OR REPLACE INTO listings (scan_id, flavor, path, access, complete) VALUES (?1, ?2, ?3, ?4, ?5)")?;
            for (path, outcome) in listings {
                insert.execute(params![
                    scan.to_string(),
                    flavor_text(path.flavor()),
                    path.as_bytes(),
                    access_text(outcome.access)?,
                    outcome.complete,
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
    /// interrupted scan resumes. Directories on a different volume than their
    /// scan root (mount points the scan deliberately skipped) are excluded.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] if SQLite fails or a stored value is invalid.
    pub fn frontier(&self, scan: ScanId) -> Result<Vec<PendingDirectory>, StoreError> {
        // Volume of each root, from the root's own stored entry.
        let mut roots_query = self.conn.prepare_cached(
            "SELECT r.flavor, r.path, e.volume FROM scan_roots r
             JOIN entries e ON e.scan_id = r.scan_id AND e.flavor = r.flavor AND e.path = r.path
             WHERE r.scan_id = ?1",
        )?;
        let roots = roots_query
            .query_map([scan.to_string()], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Vec<u8>>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })?
            .map(|row| {
                let (flavor, path, volume) = row?;
                let flavor = parse_flavor(&flavor)?;
                let path = RawPath::from_stored(flavor, path).map_err(|_| corrupt("path"))?;
                let volume = VolumeId::new(volume).map_err(|_| corrupt("volume"))?;
                Ok((path, volume))
            })
            .collect::<Result<Vec<_>, StoreError>>()?;

        let mut pending_query = self.conn.prepare_cached(
            "SELECT e.flavor, e.path, e.volume, e.file_id FROM entries e
             WHERE e.scan_id = ?1 AND e.kind = 'directory'
               AND NOT EXISTS (SELECT 1 FROM listings l
                               WHERE l.scan_id = e.scan_id AND l.flavor = e.flavor AND l.path = e.path)
             ORDER BY e.flavor, e.path",
        )?;
        let rows = pending_query.query_map([scan.to_string()], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Vec<u8>>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Vec<u8>>(3)?,
            ))
        })?;
        let mut pending = Vec::new();
        for row in rows {
            let (flavor, path, volume, file_id) = row?;
            let flavor = parse_flavor(&flavor)?;
            let path = RawPath::from_stored(flavor, path).map_err(|_| corrupt("path"))?;
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

    /// All entries of a scan, ordered by path bytes.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError::Corrupt`] for any row that fails validation, or
    /// [`StoreError`] if SQLite fails.
    pub fn entries(&self, scan: ScanId) -> Result<Vec<FilesystemEntry>, StoreError> {
        let mut query = self.conn.prepare_cached(
            "SELECT flavor, path, volume, file_id, kind, logical, allocated, private, clone_id, link_count, size_flags,
                    protection, modified_secs, modified_nanos, accessed_secs, accessed_nanos, changed_secs, changed_nanos,
                    created_secs, created_nanos, access
             FROM entries WHERE scan_id = ?1 ORDER BY flavor, path",
        )?;
        let rows = query.query_map([scan.to_string()], |row| Ok(entry_from_row(row)))?;
        rows.map(|row| row?).collect()
    }

    /// Every directory listing outcome of a scan, ordered by path bytes
    /// (roadmap 10.7): how completely each directory was read.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError::Corrupt`] for any row that fails validation, or
    /// [`StoreError`] if SQLite fails.
    pub fn listings(&self, scan: ScanId) -> Result<Vec<(RawPath, DirOutcome)>, StoreError> {
        let mut query = self.conn.prepare_cached(
            "SELECT flavor, path, access, complete FROM listings WHERE scan_id = ?1 ORDER BY flavor, path",
        )?;
        let rows = query.query_map([scan.to_string()], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Vec<u8>>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, bool>(3)?,
            ))
        })?;
        rows.map(|row| {
            let (flavor, path, access, complete) = row?;
            let path = RawPath::from_stored(parse_flavor(&flavor)?, path)
                .map_err(|_| corrupt("listings.path"))?;
            Ok((
                path,
                DirOutcome {
                    access: parse_access(&access)?,
                    complete,
                },
            ))
        })
        .collect()
    }
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

fn entry_from_row(row: &Row<'_>) -> Result<FilesystemEntry, StoreError> {
    let flavor = parse_flavor(&row.get::<_, String>(0)?)?;
    let path = RawPath::from_stored(flavor, row.get(1)?).map_err(|_| corrupt("path"))?;
    let volume = VolumeId::new(row.get::<_, String>(2)?).map_err(|_| corrupt("volume"))?;
    let file_bytes: Vec<u8> = row.get(3)?;
    let file = u128::from_be_bytes(file_bytes.try_into().map_err(|_| corrupt("file_id"))?);
    let clone_id = row
        .get::<_, Option<Vec<u8>>>(8)?
        .map(|b| {
            <[u8; 8]>::try_from(b)
                .map(|b| CloneId::new(u64::from_be_bytes(b)))
                .map_err(|_| corrupt("clone_id"))
        })
        .transpose()?;
    let link_count = u32::try_from(row.get::<_, i64>(9)?)
        .ok()
        .and_then(NonZeroU32::new)
        .ok_or_else(|| corrupt("link_count"))?;
    Ok(FilesystemEntry {
        path,
        kind: parse_kind(&row.get::<_, String>(4)?)?,
        size: SizeFacts {
            identity: FileIdentity {
                volume,
                file: FileId::new(file),
            },
            logical: ByteCount::new(from_i64("logical", row.get(5)?)?),
            allocated: ByteCount::new(from_i64("allocated", row.get(6)?)?),
            private: row
                .get::<_, Option<i64>>(7)?
                .map(|p| from_i64("private", p).map(ByteCount::new))
                .transpose()?,
            clone_id,
            link_count,
            flags: size_flags_from(row.get(10)?),
        },
        times: EntryTimes {
            modified: join_time("modified", row.get(12)?, row.get(13)?)?,
            accessed: join_time("accessed", row.get(14)?, row.get(15)?)?,
            changed: join_time("changed", row.get(16)?, row.get(17)?)?,
            created: join_time("created", row.get(18)?, row.get(19)?)?,
        },
        protection: protection_from(row.get(11)?),
        access: parse_access(&row.get::<_, String>(20)?)?,
    })
}

#[cfg(test)]
mod tests {
    use lumen_domain::{DenialReason, FailureKind};
    use lumen_testkit::Fixture;
    use proptest::prelude::*;

    use super::*;
    use crate::open_index;

    fn store() -> Result<(Fixture, IndexStore), Box<dyn std::error::Error>> {
        let fx = Fixture::build(&[])?;
        let conn = open_index(&fx.path("index.db")?)?;
        Ok((fx, IndexStore::new(conn)))
    }

    fn scan_id(n: u8) -> ScanId {
        format!("0190f1c2-7a3b-7c4d-8e5f-6a7b8c9d0e{n:02x}")
            .parse()
            .unwrap_or_else(|_| unreachable!())
    }

    fn entry(path: &[u8], file: u128) -> FilesystemEntry {
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
        expected.sort_by(|a, b| {
            (flavor_text(a.path.flavor()), a.path.as_bytes())
                .cmp(&(flavor_text(b.path.flavor()), b.path.as_bytes()))
        });
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
