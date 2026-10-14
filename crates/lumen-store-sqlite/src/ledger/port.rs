//! The [`QuarantineJournal`] port on `ledger.db` (roadmap 9.6b).

use lumen_application::ports::{Clock, JournalError, PreparedItem, QuarantineJournal, Unfinished};
use lumen_domain::QuarantineEvent;

use super::{JournalEntry, LedgerStore};

/// The quarantine journal, timestamping entries with `clock`.
#[derive(Debug)]
pub struct LedgerJournal<'a, C: Clock + ?Sized> {
    ledger: &'a mut LedgerStore,
    clock: &'a C,
}

impl<'a, C: Clock + ?Sized> LedgerJournal<'a, C> {
    /// The journal in `ledger`.
    pub fn new(ledger: &'a mut LedgerStore, clock: &'a C) -> Self {
        Self { ledger, clock }
    }
}

impl<C: Clock + ?Sized> QuarantineJournal for LedgerJournal<'_, C> {
    fn append(
        &mut self,
        operation: lumen_domain::OperationId,
        event: QuarantineEvent,
        detail: &[u8],
    ) -> Result<(), JournalError> {
        let detail: serde_json::Value = serde_json::from_slice(detail).map_err(|_| JournalError)?;
        self.ledger
            .journal(&JournalEntry {
                operation,
                event,
                at: self.clock.now(),
                detail,
            })
            .map_err(|_| JournalError)
    }

    fn unfinished(&self) -> Result<Vec<Unfinished>, JournalError> {
        self.ledger
            .unfinished()
            .map_err(|_| JournalError)?
            .into_iter()
            .map(|(state, entries)| {
                let intent = entries
                    .iter()
                    .find(|e| e.event == QuarantineEvent::Journal)
                    .ok_or(JournalError)?;
                let item = serde_json::to_vec(&intent.detail).map_err(|_| JournalError)?;
                Ok(Unfinished {
                    operation: intent.operation,
                    state,
                    item: PreparedItem(item),
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use lumen_domain::{OperationId, QuarantineState, Timestamp};
    use lumen_testkit::Fixture;

    use super::*;
    use crate::open_ledger;

    struct Epoch;

    impl Clock for Epoch {
        fn now(&self) -> Timestamp {
            Timestamp::UNIX_EPOCH
        }
    }

    #[test]
    fn appends_and_reports_unfinished_operations() -> Result<(), Box<dyn std::error::Error>> {
        let fx = Fixture::build(&[])?;
        let mut ledger = LedgerStore::new(open_ledger(&fx.path("ledger.db")?)?);
        let operation: OperationId = "0190f0d2-6c1e-7a3b-8c4d-5e6f708192a3".parse()?;
        let mut journal = LedgerJournal::new(&mut ledger, &Epoch);
        journal.append(operation, QuarantineEvent::Journal, br#"{"ino":7}"#)?;
        journal.append(operation, QuarantineEvent::Move, b"null")?;
        assert_eq!(
            journal.unfinished()?,
            [Unfinished {
                operation,
                state: QuarantineState::Moved,
                item: PreparedItem(br#"{"ino":7}"#.to_vec()),
            }]
        );
        assert_eq!(
            journal.append(operation, QuarantineEvent::Verify, b"not json"),
            Err(JournalError),
            "details must be JSON"
        );
        Ok(())
    }
}
