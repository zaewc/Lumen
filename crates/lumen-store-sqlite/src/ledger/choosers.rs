//! Who chose each quarantined item (ADR-0051): the confirmed plan's
//! [`Chooser`], recorded by the application after the item is retained, so
//! the quarantine knows which items it may remove when their retention ends.

use std::collections::BTreeMap;

use lumen_domain::{Chooser, OperationId};
use rusqlite::params;

use super::LedgerStore;
use crate::StoreError;

const fn code(chooser: Chooser) -> &'static str {
    match chooser {
        Chooser::Policy => "policy",
        Chooser::Person => "person",
    }
}

impl LedgerStore {
    /// Records that `chooser` chose `operation`'s item. A second record for
    /// the same operation is ignored: the first stands.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] if SQLite fails.
    pub fn record_chooser(
        &mut self,
        operation: OperationId,
        chooser: Chooser,
    ) -> Result<(), StoreError> {
        self.conn.execute(
            "INSERT OR IGNORE INTO quarantine_choosers (operation_id, chooser) VALUES (?1, ?2)",
            params![operation.to_string(), code(chooser)],
        )?;
        Ok(())
    }

    /// Every recorded chooser, by operation. An operation without a record
    /// is unknown, never assumed to be the policy's.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError::Corrupt`] for an invalid row, or [`StoreError`]
    /// if SQLite fails.
    pub fn choosers(&self) -> Result<BTreeMap<OperationId, Chooser>, StoreError> {
        let mut query = self
            .conn
            .prepare_cached("SELECT operation_id, chooser FROM quarantine_choosers")?;
        let rows = query.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        rows.map(|row| {
            let (operation, chooser) = row?;
            let operation = operation.parse().map_err(|_| StoreError::Corrupt {
                column: "quarantine_choosers.operation_id",
            })?;
            let chooser = match chooser.as_str() {
                "policy" => Chooser::Policy,
                "person" => Chooser::Person,
                _ => {
                    return Err(StoreError::Corrupt {
                        column: "quarantine_choosers.chooser",
                    });
                }
            };
            Ok((operation, chooser))
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
    fn choosers_are_recorded_once_and_never_changed() -> Result<(), Box<dyn std::error::Error>> {
        let fx = Fixture::build(&[])?;
        let mut ledger = LedgerStore::new(open_ledger(&fx.path("ledger.db")?)?);
        assert_eq!(ledger.choosers()?, BTreeMap::new());
        ledger.record_chooser(operation(1), Chooser::Person)?;
        ledger.record_chooser(operation(1), Chooser::Policy)?;
        ledger.record_chooser(operation(2), Chooser::Policy)?;
        assert_eq!(
            ledger.choosers()?,
            BTreeMap::from([
                (operation(1), Chooser::Person),
                (operation(2), Chooser::Policy),
            ])
        );
        let tampered = ledger
            .conn
            .execute("UPDATE quarantine_choosers SET chooser = 'policy'", []);
        assert!(tampered.is_err(), "append-only");
        Ok(())
    }
}
