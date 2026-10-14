//! Quarantining one confirmed item (roadmap 9.6, ADR-0015, ADR-0030).
//!
//! The order is fixed:
//!
//! 1. the intent is journaled durably; if that fails, nothing happens;
//! 2. the in-use probe runs again: `Yes` and `Unknown` stop here, and `Partial`
//!    passes only when the decision itself relied on a partial answer
//!    (ADR-0030 condition 4);
//! 3. the executor re-verifies, moves without replacing, verifies in the
//!    store and writes the manifest;
//! 4. the outcome is journaled: moved, verified and retained, or the failure
//!    and where it left the item.
//!
//! The journaled events always form a valid lifecycle, so a crash between any
//! two steps is found and resolved by reconciliation.

use lumen_domain::{
    FileIdentity, ItemLocation, OperationId, QuarantineEvent, QuarantineFailure, RawPath,
};

use crate::ports::{
    InUse, InUseProbe, JournalError, PreparedItem, QuarantineExecutor, QuarantineJournal,
};

/// Journal detail for events that carry none.
const NO_DETAIL: &[u8] = b"null";

/// One item of a confirmed plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfirmedItem {
    /// The operation; also the item's name in the store.
    pub operation: OperationId,
    /// What the executor prepared at confirmation time.
    pub item: PreparedItem,
    /// Where it is, for the in-use probe.
    pub path: RawPath,
    /// What it is, for the in-use probe.
    pub identity: FileIdentity,
    /// The serialized `quarantine.manifest/1`.
    pub manifest: Vec<u8>,
    /// The decision relied on a partial in-use answer (ADR-0030), so a partial
    /// answer to the re-probe is acceptable too.
    pub partial_in_use_accepted: bool,
}

/// How quarantining an item ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// In the store, verified, held for the retention period.
    Retained,
    /// Not quarantined; `left` says where the item is.
    Failed {
        /// Why.
        reason: QuarantineFailure,
        /// Untouched at the source, or in the store and restorable.
        left: ItemLocation,
    },
}

/// Quarantines `item`.
///
/// # Errors
///
/// [`JournalError`] if an entry could not be made durable. Before the move that
/// means nothing happened; after it, reconciliation finds the operation
/// unfinished and resolves it.
pub fn quarantine_item(
    executor: &dyn QuarantineExecutor,
    journal: &mut dyn QuarantineJournal,
    probe: &dyn InUseProbe,
    item: &ConfirmedItem,
) -> Result<Outcome, JournalError> {
    let operation = item.operation;
    journal.append(operation, QuarantineEvent::Journal, &item.item.0)?;
    let usable = match probe.probe(&item.path, &item.identity) {
        InUse::No => true,
        InUse::Partial { .. } => item.partial_in_use_accepted,
        InUse::Yes(_) | InUse::Unknown { .. } => false,
    };
    if !usable {
        let reason = QuarantineFailure::InUse;
        journal.append(operation, QuarantineEvent::Fail { reason }, NO_DETAIL)?;
        return Ok(Outcome::Failed {
            reason,
            left: ItemLocation::Source,
        });
    }
    match executor.quarantine(&item.item, operation, &item.manifest) {
        Ok(()) => {
            for event in [
                QuarantineEvent::Move,
                QuarantineEvent::Verify,
                QuarantineEvent::Retain,
            ] {
                journal.append(operation, event, NO_DETAIL)?;
            }
            Ok(Outcome::Retained)
        }
        Err(failure) => {
            if failure.left == ItemLocation::Quarantine {
                journal.append(operation, QuarantineEvent::Move, NO_DETAIL)?;
            }
            journal.append(
                operation,
                QuarantineEvent::Fail {
                    reason: failure.reason,
                },
                NO_DETAIL,
            )?;
            Ok(Outcome::Failed {
                reason: failure.reason,
                left: failure.left,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use lumen_domain::{FileId, QuarantineState, VolumeId};

    use super::*;
    use crate::ports::{ExecutionFailure, Located};

    struct Executor {
        result: Result<(), ExecutionFailure>,
        calls: Mutex<usize>,
    }

    impl QuarantineExecutor for Executor {
        fn prepare(
            &self,
            _root: &RawPath,
            _relative: &[Vec<u8>],
        ) -> Result<PreparedItem, ExecutionFailure> {
            Ok(PreparedItem(b"{}".to_vec()))
        }

        fn quarantine(
            &self,
            _item: &PreparedItem,
            _operation: OperationId,
            _manifest: &[u8],
        ) -> Result<(), ExecutionFailure> {
            *self
                .calls
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) += 1;
            self.result
        }

        fn locate(&self, _item: &PreparedItem, _operation: OperationId) -> Located {
            Located::Unknown
        }

        fn restore(
            &self,
            _item: &PreparedItem,
            _operation: OperationId,
        ) -> Result<crate::ports::RestoredItem, crate::ports::RestoreFailure> {
            unreachable!("quarantining restores nothing")
        }

        fn finalize(
            &self,
            _item: &PreparedItem,
            _operation: OperationId,
        ) -> Result<(), crate::ports::Removal> {
            unreachable!("quarantining finalizes nothing")
        }
    }

    #[derive(Default)]
    struct Journal {
        events: Vec<QuarantineEvent>,
        fail_from: Option<usize>,
    }

    impl QuarantineJournal for Journal {
        fn append(
            &mut self,
            _operation: OperationId,
            event: QuarantineEvent,
            _detail: &[u8],
        ) -> Result<(), JournalError> {
            if self.fail_from.is_some_and(|n| self.events.len() >= n) {
                return Err(JournalError);
            }
            self.events.push(event);
            Ok(())
        }

        fn unfinished(&self) -> Result<Vec<crate::ports::Unfinished>, JournalError> {
            Ok(Vec::new())
        }
    }

    struct Probe(InUse);

    impl InUseProbe for Probe {
        fn probe(&self, _path: &RawPath, _expected: &FileIdentity) -> InUse {
            self.0.clone()
        }
    }

    fn item(partial_accepted: bool) -> ConfirmedItem {
        ConfirmedItem {
            operation: "0190f0d2-6c1e-7a3b-8c4d-5e6f708192a3"
                .parse()
                .unwrap_or_else(|_| unreachable!()),
            item: PreparedItem(b"{\"ino\":1}".to_vec()),
            path: RawPath::from_unix_bytes(b"/Users/ana/x".to_vec())
                .unwrap_or_else(|_| unreachable!()),
            identity: FileIdentity {
                volume: VolumeId::new("v").unwrap_or_else(|_| unreachable!()),
                file: FileId::new(1),
            },
            manifest: b"{}".to_vec(),
            partial_in_use_accepted: partial_accepted,
        }
    }

    fn executor(result: Result<(), ExecutionFailure>) -> Executor {
        Executor {
            result,
            calls: Mutex::new(0),
        }
    }

    fn run(
        executor: &Executor,
        probe: InUse,
        partial_accepted: bool,
    ) -> (Result<Outcome, JournalError>, Vec<QuarantineEvent>) {
        let mut journal = Journal::default();
        let outcome = quarantine_item(
            executor,
            &mut journal,
            &Probe(probe),
            &item(partial_accepted),
        );
        (outcome, journal.events)
    }

    /// The journaled events replay as a valid lifecycle.
    fn replays(events: &[QuarantineEvent]) -> bool {
        events
            .iter()
            .try_fold(QuarantineState::Planned, |s, e| s.apply(*e))
            .is_ok()
    }

    #[test]
    fn a_free_item_is_moved_and_retained() {
        use QuarantineEvent as E;
        let exec = executor(Ok(()));
        let (outcome, events) = run(&exec, InUse::No, false);
        assert_eq!(outcome, Ok(Outcome::Retained));
        assert_eq!(events, [E::Journal, E::Move, E::Verify, E::Retain]);
        assert!(replays(&events));
    }

    #[test]
    fn items_in_use_or_unknown_are_never_moved() {
        for probe in [
            InUse::Yes(Vec::new()),
            InUse::Unknown { reason: "changed" },
            InUse::Partial { unseen: Some(3) },
        ] {
            let exec = executor(Ok(()));
            let (outcome, events) = run(&exec, probe.clone(), false);
            assert_eq!(
                outcome,
                Ok(Outcome::Failed {
                    reason: QuarantineFailure::InUse,
                    left: ItemLocation::Source
                }),
                "{probe:?}"
            );
            assert_eq!(
                *exec
                    .calls
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner),
                0
            );
            assert!(replays(&events));
        }
    }

    #[test]
    fn a_partial_answer_passes_only_when_the_decision_relied_on_one() {
        let exec = executor(Ok(()));
        let (outcome, _) = run(&exec, InUse::Partial { unseen: None }, true);
        assert_eq!(outcome, Ok(Outcome::Retained));
        let unknown = executor(Ok(()));
        let (outcome, _) = run(&unknown, InUse::Unknown { reason: "x" }, true);
        assert!(
            matches!(outcome, Ok(Outcome::Failed { .. })),
            "unknown always stops"
        );
    }

    #[test]
    fn executor_failures_are_journaled_where_they_left_the_item() {
        use QuarantineEvent as E;
        for (left, expected) in [
            (ItemLocation::Source, vec![E::Journal]),
            (ItemLocation::Quarantine, vec![E::Journal, E::Move]),
        ] {
            let reason = QuarantineFailure::VerificationFailed;
            let exec = executor(Err(ExecutionFailure { reason, left }));
            let (outcome, events) = run(&exec, InUse::No, false);
            assert_eq!(outcome, Ok(Outcome::Failed { reason, left }));
            let mut expected = expected;
            expected.push(E::Fail { reason });
            assert_eq!(events, expected);
            assert!(replays(&events));
        }
    }

    #[test]
    fn nothing_moves_without_a_durable_intent() {
        let exec = executor(Ok(()));
        let mut journal = Journal {
            fail_from: Some(0),
            ..Journal::default()
        };
        let outcome = quarantine_item(&exec, &mut journal, &Probe(InUse::No), &item(false));
        assert_eq!(outcome, Err(JournalError));
        assert_eq!(
            *exec
                .calls
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
            0
        );
    }
}
