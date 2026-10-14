//! Finalizing a retained item (roadmap 9.8b, ADR-0015).
//!
//! Only a retained item is finalized, after its retention period or at the
//! user's request (the caller decides which and says why). The item is
//! located first:
//!
//! - in the store: it is removed permanently and `Finalize` is journaled;
//! - at its source: a restore happened whose journal entry was lost, so
//!   `Restore` is journaled and nothing is removed;
//! - anywhere else: nothing happens and the user is told.
//!
//! A removal that stopped part way is not journaled; finalizing again
//! finishes it. A removal that left only the sidecar is journaled, because
//! the item itself is gone.

use lumen_domain::{OperationId, QuarantineEvent, QuarantineState};

use crate::ports::{
    JournalError, Located, PreparedItem, QuarantineExecutor, QuarantineJournal, Removal,
};

/// Why an item is finalized.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FinalizeReason {
    /// Its retention period has elapsed.
    RetentionElapsed,
    /// The user asked for it.
    UserRequest,
}

impl FinalizeReason {
    const fn as_str(self) -> &'static str {
        match self {
            Self::RetentionElapsed => "retention_elapsed",
            Self::UserRequest => "user_request",
        }
    }
}

/// How finalizing an item ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FinalizeOutcome {
    /// Permanently removed.
    Finalized,
    /// Found back at its source; journaled as restored, nothing removed.
    FoundAtSource,
    /// Not where the journal says; nothing done.
    NotFound,
    /// The removal failed; `removed` says how far it got.
    Failed {
        /// How much is gone.
        removed: Removal,
    },
    /// The item is not retained; nothing done.
    NotFinalizable(QuarantineState),
}

/// Finalizes the item of `operation`, currently in `state`.
///
/// # Errors
///
/// [`JournalError`] if the outcome could not be recorded. A finalized item
/// is then still journaled as retained; finalizing again finds it missing and
/// reports [`FinalizeOutcome::NotFound`].
pub fn finalize_item(
    executor: &dyn QuarantineExecutor,
    journal: &mut dyn QuarantineJournal,
    operation: OperationId,
    state: QuarantineState,
    item: &PreparedItem,
    reason: FinalizeReason,
) -> Result<FinalizeOutcome, JournalError> {
    if state != QuarantineState::Retained {
        return Ok(FinalizeOutcome::NotFinalizable(state));
    }
    match executor.locate(item, operation) {
        Located::InStore => {}
        Located::AtSource => {
            journal.append(
                operation,
                QuarantineEvent::Restore,
                br#"{"found_at_source":true}"#,
            )?;
            return Ok(FinalizeOutcome::FoundAtSource);
        }
        Located::Unknown => return Ok(FinalizeOutcome::NotFound),
    }
    match executor.finalize(item, operation) {
        Ok(()) | Err(Removal::Item) => {
            let detail = format!(r#"{{"reason":"{}"}}"#, reason.as_str());
            journal.append(operation, QuarantineEvent::Finalize, detail.as_bytes())?;
            Ok(FinalizeOutcome::Finalized)
        }
        Err(removed) => Ok(FinalizeOutcome::Failed { removed }),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use lumen_domain::RawPath;

    use super::*;
    use crate::ports::{ExecutionFailure, RestoreFailure, RestoredItem, Unfinished};

    struct Executor {
        located: Located,
        result: Result<(), Removal>,
        calls: Mutex<usize>,
    }

    impl QuarantineExecutor for Executor {
        fn prepare(
            &self,
            _root: &RawPath,
            _relative: &[Vec<u8>],
        ) -> Result<PreparedItem, ExecutionFailure> {
            unreachable!("finalizing prepares nothing")
        }

        fn quarantine(
            &self,
            _item: &PreparedItem,
            _operation: OperationId,
            _manifest: &[u8],
        ) -> Result<(), ExecutionFailure> {
            unreachable!("finalizing quarantines nothing")
        }

        fn locate(&self, _item: &PreparedItem, _operation: OperationId) -> Located {
            self.located
        }

        fn restore(
            &self,
            _item: &PreparedItem,
            _operation: OperationId,
        ) -> Result<RestoredItem, RestoreFailure> {
            unreachable!("finalizing restores nothing")
        }

        fn finalize(&self, _item: &PreparedItem, _operation: OperationId) -> Result<(), Removal> {
            *self
                .calls
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) += 1;
            self.result
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

    fn run(
        located: Located,
        result: Result<(), Removal>,
        state: QuarantineState,
    ) -> (FinalizeOutcome, Vec<(QuarantineEvent, Vec<u8>)>, usize) {
        let executor = Executor {
            located,
            result,
            calls: Mutex::new(0),
        };
        let mut journal = Journal::default();
        let outcome = finalize_item(
            &executor,
            &mut journal,
            "0190f0d2-6c1e-7a3b-8c4d-5e6f708192a3"
                .parse()
                .unwrap_or_else(|_| unreachable!()),
            state,
            &PreparedItem(b"{}".to_vec()),
            FinalizeReason::RetentionElapsed,
        )
        .unwrap_or_else(|e| unreachable!("{e}"));
        let calls = *executor
            .calls
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        (outcome, journal.0, calls)
    }

    #[test]
    fn a_retained_item_in_the_store_is_finalized_and_journaled() {
        for result in [Ok(()), Err(Removal::Item)] {
            let (outcome, journal, calls) =
                run(Located::InStore, result, QuarantineState::Retained);
            assert_eq!(outcome, FinalizeOutcome::Finalized);
            assert_eq!(
                journal,
                [(
                    QuarantineEvent::Finalize,
                    br#"{"reason":"retention_elapsed"}"#.to_vec()
                )]
            );
            assert_eq!(calls, 1);
        }
    }

    #[test]
    fn an_item_found_at_its_source_is_journaled_restored_and_kept() {
        let (outcome, journal, calls) = run(Located::AtSource, Ok(()), QuarantineState::Retained);
        assert_eq!(outcome, FinalizeOutcome::FoundAtSource);
        assert_eq!(
            journal.first().map(|(e, _)| *e),
            Some(QuarantineEvent::Restore)
        );
        assert_eq!(calls, 0, "nothing is removed");
    }

    #[test]
    fn an_item_not_found_or_not_removed_journals_nothing() {
        let (outcome, journal, calls) = run(Located::Unknown, Ok(()), QuarantineState::Retained);
        assert_eq!(outcome, FinalizeOutcome::NotFound);
        assert_eq!((journal.len(), calls), (0, 0));
        for removed in [Removal::Nothing, Removal::Partly] {
            let (outcome, journal, _) =
                run(Located::InStore, Err(removed), QuarantineState::Retained);
            assert_eq!(outcome, FinalizeOutcome::Failed { removed });
            assert_eq!(journal.len(), 0);
        }
    }

    #[test]
    fn only_retained_items_are_finalized() {
        for state in [
            QuarantineState::Verified,
            QuarantineState::Restored,
            QuarantineState::Finalized,
        ] {
            let (outcome, journal, calls) = run(Located::InStore, Ok(()), state);
            assert_eq!(outcome, FinalizeOutcome::NotFinalizable(state));
            assert_eq!((journal.len(), calls), (0, 0));
        }
    }
}
