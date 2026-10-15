//! The quarantine journal in `ledger.db` (roadmap 9.4, ADR-0015).
//!
//! The executor appends an intent before each move and the outcome after it.
//! On startup, [`LedgerStore::unfinished`] replays every operation through the
//! domain's quarantine state machine and returns those that stopped between
//! the intent and retention, which reconciliation must resolve by checking the
//! item at its source and in the store. A history the state machine rejects is
//! corruption, never guessed around.

use std::collections::BTreeMap;

use lumen_domain::{OperationId, QuarantineEvent, QuarantineState, Timestamp};
use rusqlite::params;
use serde::{Deserialize, Serialize};

use super::LedgerStore;
use crate::StoreError;

/// One journal entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JournalEntry {
    /// The quarantine operation (one item).
    pub operation: OperationId,
    /// What happened.
    pub event: QuarantineEvent,
    /// When it was recorded.
    pub at: Timestamp,
    /// Executor details needed to reconcile (source and store locations,
    /// expected identity), as the executor serialized them.
    pub detail: serde_json::Value,
}

fn corrupt(column: &'static str) -> StoreError {
    StoreError::Corrupt { column }
}

/// The event's name, for the lookup column.
fn event_name(event: QuarantineEvent) -> Result<String, StoreError> {
    serde_json::to_value(event)
        .ok()
        .and_then(|v| v.get("event").and_then(|e| e.as_str()).map(str::to_owned))
        .ok_or_else(|| corrupt("quarantine_journal.event"))
}

impl LedgerStore {
    /// Appends `entry`. The ledger runs with `synchronous=FULL`, so the entry is
    /// durable when this returns.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] if SQLite fails.
    pub fn journal(&mut self, entry: &JournalEntry) -> Result<(), StoreError> {
        let body = serde_json::to_vec(entry).map_err(|_| corrupt("quarantine_journal.body"))?;
        self.conn.execute(
            "INSERT INTO quarantine_journal \
             (operation_id, event, recorded_secs, recorded_nanos, body) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                entry.operation.to_string(),
                event_name(entry.event)?,
                entry.at.unix_seconds(),
                entry.at.subsec_nanos(),
                body
            ],
        )?;
        Ok(())
    }

    /// Every entry, oldest first, each checked against its lookup columns.
    fn entries(&self) -> Result<Vec<JournalEntry>, StoreError> {
        let mut query = self.conn.prepare_cached(
            "SELECT operation_id, event, body FROM quarantine_journal ORDER BY seq",
        )?;
        let rows = query.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Vec<u8>>(2)?,
            ))
        })?;
        let mut entries = Vec::new();
        for row in rows {
            let (operation, event, body) = row?;
            let entry: JournalEntry =
                serde_json::from_slice(&body).map_err(|_| corrupt("quarantine_journal.body"))?;
            if entry.operation.to_string() != operation || event_name(entry.event)? != event {
                return Err(corrupt("quarantine_journal.body"));
            }
            entries.push(entry);
        }
        Ok(entries)
    }

    /// The entries of one operation, oldest first.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError`] for a corrupt row or if SQLite fails.
    pub fn operation(&self, operation: OperationId) -> Result<Vec<JournalEntry>, StoreError> {
        Ok(self
            .entries()?
            .into_iter()
            .filter(|e| e.operation == operation)
            .collect())
    }

    /// Every journaled operation with the state its entries replay to, in
    /// operation order (roadmap 10.15): what is held in quarantine, restored,
    /// finalized or unfinished.
    ///
    /// # Errors
    ///
    /// [`StoreError::Corrupt`] if an operation's entries do not form a valid
    /// lifecycle, or [`StoreError`] if SQLite fails.
    pub fn operations(&self) -> Result<Vec<(QuarantineState, Vec<JournalEntry>)>, StoreError> {
        let mut by_operation: BTreeMap<OperationId, Vec<JournalEntry>> = BTreeMap::new();
        for entry in self.entries()? {
            by_operation.entry(entry.operation).or_default().push(entry);
        }
        by_operation
            .into_values()
            .map(|entries| {
                let state = entries
                    .iter()
                    .try_fold(QuarantineState::Planned, |state, e| {
                        state
                            .apply(e.event)
                            .map_err(|_| corrupt("quarantine_journal.event"))
                    })?;
                Ok((state, entries))
            })
            .collect()
    }

    /// Operations that stopped after their intent and before retention, with
    /// the state each reached and its entries.
    ///
    /// # Errors
    ///
    /// Returns [`StoreError::Corrupt`] if an operation's history is not a valid
    /// lifecycle, or [`StoreError`] if SQLite fails.
    pub fn unfinished(&self) -> Result<Vec<(QuarantineState, Vec<JournalEntry>)>, StoreError> {
        Ok(self
            .operations()?
            .into_iter()
            .filter(|(state, _)| {
                matches!(
                    state,
                    QuarantineState::Journaled | QuarantineState::Moved | QuarantineState::Verified
                )
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use lumen_domain::QuarantineFailure;
    use lumen_testkit::Fixture;

    use super::*;
    use crate::open_ledger;

    fn ledger() -> Result<(Fixture, LedgerStore), Box<dyn std::error::Error>> {
        let fx = Fixture::build(&[])?;
        let store = LedgerStore::new(open_ledger(&fx.path("ledger.db")?)?);
        Ok((fx, store))
    }

    fn op(n: u8) -> OperationId {
        format!("00000000-0000-4000-8000-0000000000{n:02x}")
            .parse()
            .unwrap_or_else(|_| unreachable!())
    }

    fn entry(operation: OperationId, event: QuarantineEvent) -> JournalEntry {
        JournalEntry {
            operation,
            event,
            at: Timestamp::UNIX_EPOCH,
            detail: serde_json::json!({"store_name": "item-1"}),
        }
    }

    #[test]
    fn unfinished_operations_are_found_by_replay() -> Result<(), Box<dyn std::error::Error>> {
        use QuarantineEvent as E;
        let (_fx, mut store) = ledger()?;
        let histories: [(u8, &[QuarantineEvent]); 5] = [
            (1, &[E::Journal]),
            (2, &[E::Journal, E::Move]),
            (3, &[E::Journal, E::Move, E::Verify, E::Retain]),
            (
                4,
                &[
                    E::Journal,
                    E::Fail {
                        reason: QuarantineFailure::IdentityMismatch,
                    },
                ],
            ),
            (5, &[E::Journal, E::Move, E::Verify]),
        ];
        for (n, events) in histories {
            for event in events {
                store.journal(&entry(op(n), *event))?;
            }
        }
        let unfinished: Vec<(QuarantineState, OperationId)> = store
            .unfinished()?
            .into_iter()
            .map(|(state, entries)| (state, entries[0].operation))
            .collect();
        assert_eq!(
            unfinished,
            [
                (QuarantineState::Journaled, op(1)),
                (QuarantineState::Moved, op(2)),
                (QuarantineState::Verified, op(5)),
            ]
        );
        assert_eq!(store.operation(op(3))?.len(), 4);
        let states: Vec<(OperationId, QuarantineState)> = store
            .operations()?
            .into_iter()
            .map(|(state, entries)| (entries[0].operation, state))
            .collect();
        assert_eq!(states.len(), 5, "every operation, finished or not");
        assert_eq!(states[2], (op(3), QuarantineState::Retained));
        assert!(matches!(states[3].1, QuarantineState::Failed { .. }));
        Ok(())
    }

    #[test]
    fn invalid_histories_and_edits_are_rejected() -> Result<(), Box<dyn std::error::Error>> {
        let (_fx, mut store) = ledger()?;
        store.journal(&entry(op(1), QuarantineEvent::Move))?;
        assert!(matches!(
            store.unfinished(),
            Err(StoreError::Corrupt { .. })
        ));
        assert!(
            store
                .conn
                .execute("UPDATE quarantine_journal SET event = 'journal'", [])
                .is_err()
        );
        assert!(
            store
                .conn
                .execute("DELETE FROM quarantine_journal", [])
                .is_err()
        );
        Ok(())
    }
}
