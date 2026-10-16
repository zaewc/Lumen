//! Restoring a quarantined item (roadmap 9.7b, ADR-0015).
//!
//! Only a retained item, or one that failed after reaching the store (such as
//! reconciliation's roll-backs), can be restored; the lifecycle decides. The
//! executor moves it back without replacing anything. `Restore` is journaled
//! exactly when the item is back at its source, even if the check after the
//! move failed, so the journal always says where the item is. An item still in
//! the store keeps its state, and the reason goes to the user (a taken name is
//! theirs to resolve). Once `Restore` is journaled, the executor forgets the
//! operation's manifest sidecar; one that stays is harmless, because the
//! journal is the record.

use lumen_domain::{ItemLocation, OperationId, QuarantineEvent, QuarantineState};

use crate::ports::{
    JournalError, NotRestored, PreparedItem, QuarantineExecutor, QuarantineJournal,
};

/// How a restore ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestoreOutcome {
    /// Back at its original location.
    Restored {
        /// The original parent had been recreated.
        parent_replaced: bool,
        /// The item matched its record after the move.
        verified: bool,
    },
    /// Still in the store.
    NotRestored(NotRestored),
    /// The item is not in a state that can be restored; nothing was done.
    NotRestorable(QuarantineState),
}

/// Restores the item of `operation`, currently in `state`.
///
/// # Errors
///
/// [`JournalError`] if the restore could not be recorded. The journal then
/// still says the item is in the store; finalization looks for it there first
/// and never removes what it does not find (roadmap 9.8).
pub fn restore_item(
    executor: &dyn QuarantineExecutor,
    journal: &mut dyn QuarantineJournal,
    operation: OperationId,
    state: QuarantineState,
    item: &PreparedItem,
) -> Result<RestoreOutcome, JournalError> {
    if state.apply(QuarantineEvent::Restore).is_err() {
        return Ok(RestoreOutcome::NotRestorable(state));
    }
    let (parent_replaced, verified) = match executor.restore(item, operation) {
        Ok(restored) => (restored.parent_replaced, true),
        Err(failure) if failure.left == ItemLocation::Source => (false, false),
        Err(failure) => return Ok(RestoreOutcome::NotRestored(failure.reason)),
    };
    let detail = format!(r#"{{"parent_replaced":{parent_replaced},"verified":{verified}}}"#);
    journal.append(operation, QuarantineEvent::Restore, detail.as_bytes())?;
    let _forgotten: bool = executor.forget(item, operation);
    Ok(RestoreOutcome::Restored {
        parent_replaced,
        verified,
    })
}

#[cfg(test)]
mod tests {
    use lumen_domain::{QuarantineFailure, RawPath};

    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;
    use crate::ports::{ExecutionFailure, Located, RestoreFailure, RestoredItem, Unfinished};

    struct Executor(Result<RestoredItem, RestoreFailure>, AtomicUsize);

    impl QuarantineExecutor for Executor {
        fn prepare(
            &self,
            _root: &RawPath,
            _relative: &[Vec<u8>],
        ) -> Result<PreparedItem, ExecutionFailure> {
            unreachable!("restore prepares nothing")
        }

        fn quarantine(
            &self,
            _item: &PreparedItem,
            _operation: OperationId,
            _manifest: &[u8],
        ) -> Result<(), ExecutionFailure> {
            unreachable!("restore quarantines nothing")
        }

        fn locate(&self, _item: &PreparedItem, _operation: OperationId) -> Located {
            Located::Unknown
        }

        fn restore(
            &self,
            _item: &PreparedItem,
            _operation: OperationId,
        ) -> Result<RestoredItem, RestoreFailure> {
            self.0
        }

        fn finalize(
            &self,
            _item: &PreparedItem,
            _operation: OperationId,
        ) -> Result<(), crate::ports::Removal> {
            unreachable!("restoring finalizes nothing")
        }

        fn forget(&self, _item: &PreparedItem, _operation: OperationId) -> bool {
            self.1.fetch_add(1, Ordering::Relaxed);
            true
        }
    }

    #[derive(Default)]
    struct Journal(Vec<(QuarantineEvent, Vec<u8>)>);

    impl QuarantineJournal for Journal {
        fn append(
            &mut self,
            _operation: OperationId,
            event: QuarantineEvent,
            detail: &[u8],
        ) -> Result<(), JournalError> {
            self.0.push((event, detail.to_vec()));
            Ok(())
        }

        fn unfinished(&self) -> Result<Vec<Unfinished>, JournalError> {
            Ok(Vec::new())
        }
    }

    fn op() -> OperationId {
        "0190f0d2-6c1e-7a3b-8c4d-5e6f708192a3"
            .parse()
            .unwrap_or_else(|_| unreachable!())
    }

    fn run(
        result: Result<RestoredItem, RestoreFailure>,
        state: QuarantineState,
    ) -> (RestoreOutcome, Journal) {
        let mut journal = Journal::default();
        let executor = Executor(result, AtomicUsize::new(0));
        let outcome = restore_item(
            &executor,
            &mut journal,
            op(),
            state,
            &PreparedItem(b"{}".to_vec()),
        )
        .unwrap_or_else(|e| unreachable!("{e}"));
        let restored = matches!(outcome, RestoreOutcome::Restored { .. });
        assert_eq!(
            executor.1.load(Ordering::Relaxed),
            usize::from(restored),
            "the sidecar is forgotten exactly when the restore is journaled"
        );
        (outcome, journal)
    }

    const FAILED_IN_STORE: QuarantineState = QuarantineState::Failed {
        reason: QuarantineFailure::VerificationFailed,
        at: ItemLocation::Quarantine,
    };

    #[test]
    fn retained_and_rolled_back_items_are_restored_and_journaled() {
        for state in [QuarantineState::Retained, FAILED_IN_STORE] {
            let (outcome, journal) = run(
                Ok(RestoredItem {
                    parent_replaced: true,
                }),
                state,
            );
            assert_eq!(
                outcome,
                RestoreOutcome::Restored {
                    parent_replaced: true,
                    verified: true
                }
            );
            assert_eq!(
                journal.0,
                [(
                    QuarantineEvent::Restore,
                    br#"{"parent_replaced":true,"verified":true}"#.to_vec()
                )]
            );
        }
    }

    #[test]
    fn an_item_back_at_its_source_is_journaled_even_if_unverified() {
        let (outcome, journal) = run(
            Err(RestoreFailure {
                reason: NotRestored::Failed,
                left: ItemLocation::Source,
            }),
            QuarantineState::Retained,
        );
        assert_eq!(
            outcome,
            RestoreOutcome::Restored {
                parent_replaced: false,
                verified: false
            }
        );
        assert_eq!(journal.0.len(), 1);
    }

    #[test]
    fn an_item_still_in_the_store_keeps_its_state() {
        let (outcome, journal) = run(
            Err(RestoreFailure {
                reason: NotRestored::NameTaken,
                left: ItemLocation::Quarantine,
            }),
            QuarantineState::Retained,
        );
        assert_eq!(outcome, RestoreOutcome::NotRestored(NotRestored::NameTaken));
        assert_eq!(journal.0, []);
    }

    #[test]
    fn items_not_in_the_store_are_never_restored() {
        for state in [
            QuarantineState::Journaled,
            QuarantineState::Verified,
            QuarantineState::Restored,
            QuarantineState::Finalized,
        ] {
            let (outcome, journal) = run(
                Ok(RestoredItem {
                    parent_replaced: false,
                }),
                state,
            );
            assert_eq!(outcome, RestoreOutcome::NotRestorable(state));
            assert_eq!(journal.0, []);
        }
    }
}
