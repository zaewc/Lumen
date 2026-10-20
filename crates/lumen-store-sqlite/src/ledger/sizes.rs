//! What each quarantined item will free once removed for good (roadmap 13):
//! the confirmed plan's estimate, recorded by the application after the item
//! is retained, so the quarantine can show the space it holds.

use std::collections::BTreeMap;

use lumen_domain::{ByteCount, OperationId};
use rusqlite::params;

use super::LedgerStore;
use crate::StoreError;

impl LedgerStore {
    /// Records that `operation`'s item will free `reclaim`. A second record
    /// for the same operation is ignored: the first estimate stands.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] if SQLite fails.
    pub fn record_reclaim(
        &mut self,
        operation: OperationId,
        reclaim: ByteCount,
    ) -> Result<(), StoreError> {
        let bytes = i64::try_from(reclaim.get()).map_err(|_| StoreError::Corrupt {
            column: "quarantine_sizes.reclaim_bytes",
        })?;
        self.conn.execute(
            "INSERT OR IGNORE INTO quarantine_sizes (operation_id, reclaim_bytes) VALUES (?1, ?2)",
            params![operation.to_string(), bytes],
        )?;
        Ok(())
    }

    /// Every recorded estimate, by operation.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError::Corrupt`] for an invalid row, or [`StoreError`]
    /// if SQLite fails.
    pub fn reclaims(&self) -> Result<BTreeMap<OperationId, ByteCount>, StoreError> {
        let mut query = self
            .conn
            .prepare_cached("SELECT operation_id, reclaim_bytes FROM quarantine_sizes")?;
        let rows = query.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?;
        rows.map(|row| {
            let (operation, bytes) = row?;
            let operation = operation.parse().map_err(|_| StoreError::Corrupt {
                column: "quarantine_sizes.operation_id",
            })?;
            let bytes = u64::try_from(bytes).map_err(|_| StoreError::Corrupt {
                column: "quarantine_sizes.reclaim_bytes",
            })?;
            Ok((operation, ByteCount::new(bytes)))
        })
        .collect()
    }
}

#[cfg(test)]
mod tests {
    use lumen_testkit::Fixture;

    use super::*;
    use crate::open_ledger;

    fn operation(n: u8) -> OperationId {
        format!("0190f0d2-6c1e-7a3b-8c4d-5e6f7081920{n}")
            .parse()
            .unwrap_or_else(|_| unreachable!())
    }

    #[test]
    fn estimates_are_recorded_once_and_never_changed() -> Result<(), Box<dyn std::error::Error>> {
        let fx = Fixture::build(&[])?;
        let mut ledger = LedgerStore::new(open_ledger(&fx.path("ledger.db")?)?);
        assert_eq!(ledger.reclaims()?, BTreeMap::new());
        ledger.record_reclaim(operation(1), ByteCount::new(4_200))?;
        ledger.record_reclaim(operation(1), ByteCount::new(9))?;
        ledger.record_reclaim(operation(2), ByteCount::ZERO)?;
        assert_eq!(
            ledger.reclaims()?,
            BTreeMap::from([
                (operation(1), ByteCount::new(4_200)),
                (operation(2), ByteCount::ZERO),
            ])
        );
        let tampered = ledger
            .conn
            .execute("UPDATE quarantine_sizes SET reclaim_bytes = 1", []);
        assert!(tampered.is_err(), "append-only");
        Ok(())
    }
}
