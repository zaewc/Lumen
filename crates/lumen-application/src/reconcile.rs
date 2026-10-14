//! Reconciling interrupted quarantine operations on startup (roadmap 9.6c,
//! ADR-0015).
//!
//! An operation is interrupted when its journal stops after the intent and
//! before retention (a crash, power loss, a killed process). Each one is
//! resolved by locating its item and journaling what really happened:
//!
//! | Journal reached | Item found | Journaled now | Result |
//! | --- | --- | --- | --- |
//! | intent | at its source | `Fail` | untouched |
//! | intent | in the store | `Move`, `Fail` | in the store, to roll back |
//! | moved | in the store | `Fail` | in the store, to roll back |
//! | verified | in the store | `Retain` | retained |
//!
//! An item moved but never journaled as verified may lack its manifest
//! sidecar (the executor writes it after the move), so it is not retained: it
//! fails in the store, where restore rolls it back (ADR-0015). Only an item
//! whose verification was journaled is retained.
//!
//! Anything else (unknown location, or a location the journal contradicts) is
//! left untouched and reported for the user's attention: reconciliation never
//! guesses and never moves anything.

use lumen_domain::{OperationId, QuarantineEvent, QuarantineFailure, QuarantineState};

use crate::ports::{JournalError, Located, QuarantineExecutor, QuarantineJournal};

/// Journal detail for events that carry none.
const NO_DETAIL: &[u8] = b"null";

/// How an interrupted, unverified operation fails.
const INTERRUPTED: QuarantineEvent = QuarantineEvent::Fail {
    reason: QuarantineFailure::VerificationFailed,
};

/// What reconciliation did with one operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resolution {
    /// The move never happened; the item is at its source.
    NotMoved,
    /// The move happened but was never verified; the item is in the store,
    /// failed, waiting to be rolled back.
    RollBack,
    /// Verification had completed; the item is now retained.
    Retained,
    /// The item could not be located consistently; it needs the user.
    NeedsAttention,
}

/// Resolves every interrupted operation.
///
/// # Errors
///
/// [`JournalError`] if the journal cannot be read or written; operations
/// already resolved stay resolved, the rest stay interrupted for next time.
pub fn reconcile(
    executor: &dyn QuarantineExecutor,
    journal: &mut dyn QuarantineJournal,
) -> Result<Vec<(OperationId, Resolution)>, JournalError> {
    let mut resolved = Vec::new();
    for unfinished in journal.unfinished()? {
        let located = executor.locate(&unfinished.item, unfinished.operation);
        let (events, resolution): (&[QuarantineEvent], _) = match (unfinished.state, located) {
            (QuarantineState::Journaled, Located::AtSource) => {
                (&[INTERRUPTED], Resolution::NotMoved)
            }
            (QuarantineState::Journaled, Located::InStore) => {
                (&[QuarantineEvent::Move, INTERRUPTED], Resolution::RollBack)
            }
            (QuarantineState::Moved, Located::InStore) => (&[INTERRUPTED], Resolution::RollBack),
            (QuarantineState::Verified, Located::InStore) => {
                (&[QuarantineEvent::Retain], Resolution::Retained)
            }
            _ => (&[], Resolution::NeedsAttention),
        };
        for event in events {
            journal.append(unfinished.operation, *event, NO_DETAIL)?;
        }
        resolved.push((unfinished.operation, resolution));
    }
    Ok(resolved)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use lumen_domain::{ItemLocation, RawPath};

    use super::*;
    use crate::ports::{ExecutionFailure, PreparedItem, Unfinished};

    /// Locates items from a fixed table.
    struct Locator(BTreeMap<OperationId, Located>);

    impl QuarantineExecutor for Locator {
        fn prepare(
            &self,
            _root: &RawPath,
            _relative: &[Vec<u8>],
        ) -> Result<PreparedItem, ExecutionFailure> {
            Err(ExecutionFailure {
                reason: QuarantineFailure::IdentityMismatch,
                left: ItemLocation::Source,
            })
        }

        fn quarantine(
            &self,
            _item: &PreparedItem,
            _operation: OperationId,
            _manifest: &[u8],
        ) -> Result<(), ExecutionFailure> {
            unreachable!("reconciliation never moves anything")
        }

        fn locate(&self, _item: &PreparedItem, operation: OperationId) -> Located {
            self.0.get(&operation).copied().unwrap_or(Located::Unknown)
        }
    }

    /// A journal holding complete histories in memory.
    #[derive(Default)]
    struct Journal(BTreeMap<OperationId, Vec<QuarantineEvent>>);

    impl QuarantineJournal for Journal {
        fn append(
            &mut self,
            operation: OperationId,
            event: QuarantineEvent,
            _detail: &[u8],
        ) -> Result<(), JournalError> {
            self.0.entry(operation).or_default().push(event);
            Ok(())
        }

        fn unfinished(&self) -> Result<Vec<Unfinished>, JournalError> {
            Ok(self
                .0
                .iter()
                .filter_map(|(operation, events)| {
                    let state = events
                        .iter()
                        .try_fold(QuarantineState::Planned, |s, e| s.apply(*e))
                        .ok()?;
                    matches!(
                        state,
                        QuarantineState::Journaled
                            | QuarantineState::Moved
                            | QuarantineState::Verified
                    )
                    .then(|| Unfinished {
                        operation: *operation,
                        state,
                        item: PreparedItem(b"{}".to_vec()),
                    })
                })
                .collect())
        }
    }

    fn op(n: u8) -> OperationId {
        format!("0190f0d2-6c1e-7a3b-8c4d-5e6f708192{n:02x}")
            .parse()
            .unwrap_or_else(|_| unreachable!())
    }

    #[test]
    fn interrupted_operations_are_resolved_from_where_items_are() {
        use QuarantineEvent as E;
        let mut journal = Journal::default();
        let histories: [(u8, &[QuarantineEvent], Located); 6] = [
            (1, &[E::Journal], Located::AtSource),
            (2, &[E::Journal], Located::InStore),
            (3, &[E::Journal, E::Move], Located::InStore),
            (4, &[E::Journal, E::Move, E::Verify], Located::InStore),
            (5, &[E::Journal], Located::Unknown),
            (6, &[E::Journal, E::Move], Located::AtSource),
        ];
        let mut located = BTreeMap::new();
        for (n, events, at) in histories {
            journal.0.insert(op(n), events.to_vec());
            located.insert(op(n), at);
        }
        let resolved =
            reconcile(&Locator(located), &mut journal).unwrap_or_else(|e| unreachable!("{e}"));
        assert_eq!(
            resolved,
            [
                (op(1), Resolution::NotMoved),
                (op(2), Resolution::RollBack),
                (op(3), Resolution::RollBack),
                (op(4), Resolution::Retained),
                (op(5), Resolution::NeedsAttention),
                (op(6), Resolution::NeedsAttention),
            ]
        );
        // Every resolved history replays as a valid lifecycle; only the
        // unresolved stay unfinished.
        let unfinished: Vec<OperationId> = journal
            .unfinished()
            .unwrap_or_default()
            .into_iter()
            .map(|u| u.operation)
            .collect();
        assert_eq!(unfinished, [op(5), op(6)]);
        let state = |n| {
            journal
                .0
                .get(&op(n))
                .and_then(|events: &Vec<QuarantineEvent>| {
                    events
                        .iter()
                        .try_fold(QuarantineState::Planned, |s, e| s.apply(*e))
                        .ok()
                })
        };
        assert_eq!(
            state(1).map(QuarantineState::item_location),
            Some(ItemLocation::Source)
        );
        for n in [2, 3] {
            assert!(
                matches!(state(n), Some(s) if s.apply(E::Restore).is_ok()),
                "op {n} can be rolled back"
            );
        }
        assert_eq!(state(4), Some(QuarantineState::Retained));
    }

    #[test]
    fn running_twice_changes_nothing_more() {
        use QuarantineEvent as E;
        let mut journal = Journal::default();
        journal
            .0
            .insert(op(1), vec![E::Journal, E::Move, E::Verify]);
        let locator = Locator(BTreeMap::from([(op(1), Located::InStore)]));
        let first = reconcile(&locator, &mut journal).unwrap_or_else(|e| unreachable!("{e}"));
        let second = reconcile(&locator, &mut journal).unwrap_or_else(|e| unreachable!("{e}"));
        assert_eq!(first, [(op(1), Resolution::Retained)]);
        assert_eq!(second, []);
    }
}
