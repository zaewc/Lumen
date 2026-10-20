//! Keeping `index.db` small (roadmap 13): only the latest completed scan's
//! entries are ever read (an assessment decides from it), so a completed
//! scan replaces the rows of every earlier finished one. Scan records, root
//! snapshots, plans and the evidence graph are kept.

use lumen_domain::{ScanId, Timestamp};
use rusqlite::params;

use super::IndexStore;
use crate::StoreError;

/// What [`IndexStore::prune`] removed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Pruned {
    /// Earlier scans whose entries and listings were removed.
    pub scans: u64,
    /// Scans left `running` or `paused` since before the cut-off, now `failed`.
    pub abandoned: u64,
}

impl IndexStore {
    /// Removes the entries and listings of every finished scan except `keep`,
    /// after marking scans still `running` or `paused` that started before
    /// `abandoned_before` as `failed` (nothing resumes them; a `failed` scan is
    /// never assessed). Scans that started later are left alone: another
    /// process may still be writing them.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] if SQLite fails; nothing is removed then.
    pub fn prune(
        &mut self,
        keep: ScanId,
        abandoned_before: Timestamp,
    ) -> Result<Pruned, StoreError> {
        let tx = self.conn.transaction()?;
        let abandoned = tx.execute(
            "UPDATE scans SET state = 'failed'
             WHERE id != ?1 AND state IN ('created', 'running', 'paused')
               AND (started_secs < ?2 OR (started_secs = ?2 AND started_nanos < ?3))",
            params![
                keep.to_string(),
                abandoned_before.unix_seconds(),
                abandoned_before.subsec_nanos()
            ],
        )?;
        let pruned = tx.query_row(
            "SELECT count(DISTINCT d.scan) FROM dirs d JOIN scans s ON s.key = d.scan
             WHERE s.id != ?1 AND s.state IN ('completed', 'cancelled', 'failed')",
            [keep.to_string()],
            |row| row.get::<_, i64>(0),
        )?;
        tx.execute(
            "DELETE FROM dirs WHERE scan IN (
                 SELECT key FROM scans
                 WHERE id != ?1 AND state IN ('completed', 'cancelled', 'failed'))",
            [keep.to_string()],
        )?;
        tx.commit()?;
        // The cache holds the ids of the scan being written; never stale ones.
        self.dirs.clear();
        Ok(Pruned {
            scans: u64::try_from(pruned).unwrap_or(0),
            abandoned: u64::try_from(abandoned).unwrap_or(0),
        })
    }

    /// Bytes of free pages in the file: space a [`IndexStore::vacuum`] would
    /// give back to the file system.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] if SQLite fails.
    pub fn free_bytes(&self) -> Result<u64, StoreError> {
        let pages: i64 = self
            .conn
            .query_row("PRAGMA freelist_count", [], |row| row.get(0))?;
        let size: i64 = self
            .conn
            .query_row("PRAGMA page_size", [], |row| row.get(0))?;
        Ok(u64::try_from(pages.saturating_mul(size)).unwrap_or(0))
    }

    /// Bytes the file holds in use: what a [`IndexStore::vacuum`] rewrites,
    /// and about the extra space it needs while it runs.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] if SQLite fails.
    pub fn used_bytes(&self) -> Result<u64, StoreError> {
        let pages: i64 = self
            .conn
            .query_row("PRAGMA page_count", [], |row| row.get(0))?;
        let size: i64 = self
            .conn
            .query_row("PRAGMA page_size", [], |row| row.get(0))?;
        let total = u64::try_from(pages.saturating_mul(size)).unwrap_or(0);
        Ok(total.saturating_sub(self.free_bytes()?))
    }

    /// Rewrites the file without its free pages, returning them to the file
    /// system. Needs about [`IndexStore::used_bytes`] of free disk space while
    /// it runs; check before calling.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] if SQLite fails; the file is unchanged then.
    pub fn vacuum(&mut self) -> Result<(), StoreError> {
        self.conn.execute_batch("VACUUM")?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use lumen_domain::{ScanState, Timestamp};

    use super::*;
    use crate::index::tests::{entry, scan_id, store};

    fn at(secs: i64) -> Timestamp {
        Timestamp::from_unix(secs, 0).unwrap_or_else(|_| unreachable!())
    }

    #[test]
    fn a_completed_scan_replaces_earlier_rows_but_not_a_live_one()
    -> Result<(), Box<dyn std::error::Error>> {
        let (_fx, mut store) = store()?;
        // 1: completed earlier; 2: abandoned long ago; 3: running now
        // (another process); 4: the scan that just completed.
        for (n, started, state) in [
            (1, 10, Some(ScanState::Completed)),
            (2, 20, None),
            (3, 10_000, None),
            (4, 30, Some(ScanState::Completed)),
        ] {
            store.begin_scan(scan_id(n), at(started), &[])?;
            store.insert_entries(
                scan_id(n),
                &[entry(format!("/r{n}/a").as_bytes(), u128::from(n))],
            )?;
            if let Some(state) = state {
                store.finish_scan(scan_id(n), state, at(started + 1))?;
            }
        }
        let pruned = store.prune(scan_id(4), at(5_000))?;
        assert_eq!(
            pruned,
            Pruned {
                scans: 2,
                abandoned: 1
            }
        );
        assert_eq!(store.entries(scan_id(1))?.len(), 0);
        assert_eq!(store.entries(scan_id(2))?.len(), 0);
        assert_eq!(store.scan_state(scan_id(2))?, Some(ScanState::Failed));
        assert_eq!(
            store.entries(scan_id(3))?.len(),
            1,
            "a live scan is untouched"
        );
        assert_eq!(store.scan_state(scan_id(3))?, Some(ScanState::Running));
        assert_eq!(store.entries(scan_id(4))?.len(), 1, "the kept scan");
        assert_eq!(store.latest_completed_scan()?, Some(scan_id(4)));
        assert_eq!(
            store.prune(scan_id(4), at(5_000))?,
            Pruned::default(),
            "idempotent"
        );
        Ok(())
    }

    #[test]
    fn vacuum_gives_free_pages_back() -> Result<(), Box<dyn std::error::Error>> {
        let (_fx, mut store) = store()?;
        store.begin_scan(scan_id(1), at(1), &[])?;
        let many: Vec<_> = (0..3_000_u32)
            .map(|i| entry(format!("/r/{i:0>200}").as_bytes(), u128::from(i)))
            .collect();
        store.insert_entries(scan_id(1), &many)?;
        store.finish_scan(scan_id(1), ScanState::Completed, at(2))?;
        store.begin_scan(scan_id(2), at(3), &[])?;
        store.finish_scan(scan_id(2), ScanState::Completed, at(4))?;
        store.prune(scan_id(2), at(0))?;
        assert!(store.free_bytes()? > 500_000, "{}", store.free_bytes()?);
        store.vacuum()?;
        assert_eq!(store.free_bytes()?, 0);
        assert!(store.used_bytes()? > 0);
        Ok(())
    }
}
