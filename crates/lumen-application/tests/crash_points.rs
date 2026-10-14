//! A crash at every step (roadmap 9.9c, ADR-0015).
//!
//! A model of one item's physical location stands in for the filesystem, and
//! a journal that stops accepting entries after `k` of them stands in for a
//! crash. For every `k`, the run is interrupted there, the process "restarts"
//! with a working journal, and the startup and maintenance use cases run. The
//! journal must then replay as a valid lifecycle whose location is where the
//! item physically is: never a record of an item that is elsewhere.

use std::sync::Mutex;

use lumen_application::finalize::{FinalizeOutcome, FinalizeReason, finalize_item};
use lumen_application::ports::{
    ExecutionFailure, InUse, InUseProbe, JournalError, Located, PreparedItem, QuarantineExecutor,
    QuarantineJournal, Removal, RestoreFailure, RestoredItem, Unfinished,
};
use lumen_application::quarantine::{ConfirmedItem, quarantine_item};
use lumen_application::reconcile::reconcile;
use lumen_application::restore::restore_item;
use lumen_domain::{
    FileId, FileIdentity, ItemLocation, OperationId, QuarantineEvent, QuarantineFailure,
    QuarantineState, RawPath, VolumeId,
};

/// Where the item physically is, changed only by the executor.
struct World {
    at: Mutex<ItemLocation>,
    /// The executor's own failure, if any, and where it leaves the item.
    failure: Option<ExecutionFailure>,
}

impl World {
    fn new(failure: Option<ExecutionFailure>) -> Self {
        Self {
            at: Mutex::new(ItemLocation::Source),
            failure,
        }
    }

    fn at(&self) -> ItemLocation {
        *self
            .at
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn put(&self, at: ItemLocation) {
        *self
            .at
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = at;
    }
}

impl QuarantineExecutor for World {
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
        if let Some(failure) = self.failure {
            self.put(failure.left);
            Err(failure)
        } else {
            self.put(ItemLocation::Quarantine);
            Ok(())
        }
    }

    fn locate(&self, _item: &PreparedItem, _operation: OperationId) -> Located {
        match self.at() {
            ItemLocation::Source => Located::AtSource,
            ItemLocation::Quarantine => Located::InStore,
            ItemLocation::Gone => Located::Unknown,
        }
    }

    fn restore(
        &self,
        _item: &PreparedItem,
        _operation: OperationId,
    ) -> Result<RestoredItem, RestoreFailure> {
        self.put(ItemLocation::Source);
        Ok(RestoredItem {
            parent_replaced: false,
        })
    }

    fn finalize(&self, _item: &PreparedItem, _operation: OperationId) -> Result<(), Removal> {
        self.put(ItemLocation::Gone);
        Ok(())
    }
}

/// A durable journal that crashes (refuses every entry) once `limit` entries
/// have been written, until the process restarts.
struct Journal {
    events: Vec<QuarantineEvent>,
    limit: Option<usize>,
    item: PreparedItem,
    operation: OperationId,
}

impl Journal {
    fn crashing_after(limit: usize, operation: OperationId) -> Self {
        Self {
            events: Vec::new(),
            limit: Some(limit),
            item: PreparedItem(b"{}".to_vec()),
            operation,
        }
    }

    fn restart(&mut self) {
        self.limit = None;
    }

    fn state(&self) -> QuarantineState {
        self.events
            .iter()
            .try_fold(QuarantineState::Planned, |s, e| s.apply(*e))
            .unwrap_or_else(|e| unreachable!("the journal must replay: {e:?}"))
    }
}

impl QuarantineJournal for Journal {
    fn append(
        &mut self,
        _operation: OperationId,
        event: QuarantineEvent,
        _detail: &[u8],
    ) -> Result<(), JournalError> {
        if self.limit.is_some_and(|n| self.events.len() >= n) {
            return Err(JournalError);
        }
        self.events.push(event);
        Ok(())
    }

    fn unfinished(&self) -> Result<Vec<Unfinished>, JournalError> {
        let state = self.state();
        Ok(matches!(
            state,
            QuarantineState::Journaled | QuarantineState::Moved | QuarantineState::Verified
        )
        .then(|| Unfinished {
            operation: self.operation,
            state,
            item: self.item.clone(),
        })
        .into_iter()
        .collect())
    }
}

struct Free;

impl InUseProbe for Free {
    fn probe(&self, _path: &RawPath, _expected: &FileIdentity) -> InUse {
        InUse::No
    }
}

fn operation() -> OperationId {
    "0190f0d2-6c1e-7a3b-8c4d-5e6f708192a3"
        .parse()
        .unwrap_or_else(|_| unreachable!())
}

fn confirmed() -> ConfirmedItem {
    ConfirmedItem {
        operation: operation(),
        item: PreparedItem(b"{}".to_vec()),
        path: RawPath::from_unix_bytes(b"/fixture/item".to_vec())
            .unwrap_or_else(|_| unreachable!()),
        identity: FileIdentity {
            volume: VolumeId::new("v").unwrap_or_else(|_| unreachable!()),
            file: FileId::new(1),
        },
        manifest: b"{}".to_vec(),
        partial_in_use_accepted: false,
    }
}

/// The journal agrees with the world.
fn consistent(journal: &Journal, world: &World) -> bool {
    journal.state().item_location() == world.at()
}

#[test]
fn a_crash_while_quarantining_is_always_resolved_consistently() {
    let failures = [
        None,
        Some(ExecutionFailure {
            reason: QuarantineFailure::MoveFailed,
            left: ItemLocation::Source,
        }),
        Some(ExecutionFailure {
            reason: QuarantineFailure::VerificationFailed,
            left: ItemLocation::Quarantine,
        }),
    ];
    for failure in failures {
        for limit in 0..=4 {
            let world = World::new(failure);
            let mut journal = Journal::crashing_after(limit, operation());
            // The run; an error is the crash.
            let _crashed = quarantine_item(&world, &mut journal, &Free, &confirmed());
            journal.restart();
            reconcile(&world, &mut journal).unwrap_or_else(|e| unreachable!("{e}"));
            assert!(
                consistent(&journal, &world),
                "{failure:?} crash after {limit}: {:?} vs {:?}",
                journal.events,
                world.at()
            );
            assert_eq!(journal.unfinished().map(|u| u.len()), Ok(0), "nothing left");
        }
    }
}

#[test]
fn a_crash_while_restoring_is_corrected_before_anything_is_removed() {
    use QuarantineEvent as E;
    // Four entries are already written; the restore's own entry is the fifth.
    for (limit, expected) in [
        (4, FinalizeOutcome::FoundAtSource),
        (
            5,
            FinalizeOutcome::NotFinalizable(QuarantineState::Restored),
        ),
    ] {
        let world = World::new(None);
        world.put(ItemLocation::Quarantine);
        let mut journal = Journal::crashing_after(limit, operation());
        journal.events = vec![E::Journal, E::Move, E::Verify, E::Retain];
        let retained = journal.state();
        let restored = restore_item(
            &world,
            &mut journal,
            operation(),
            retained,
            &PreparedItem(b"{}".to_vec()),
        );
        assert_eq!(
            restored.is_ok(),
            limit == 5,
            "the restore's entry is lost only in the crash"
        );
        assert!(matches!(world.at(), ItemLocation::Source));
        journal.restart();
        // Retention later elapses; finalization must find the truth first.
        let state = journal.state();
        let outcome = finalize_item(
            &world,
            &mut journal,
            operation(),
            state,
            &PreparedItem(b"{}".to_vec()),
            FinalizeReason::RetentionElapsed,
        )
        .unwrap_or_else(|e| unreachable!("{e}"));
        assert_eq!(outcome, expected);
        assert_eq!(
            world.at(),
            ItemLocation::Source,
            "a restored item is never removed"
        );
        assert!(
            consistent(&journal, &world),
            "crash after {limit}: {:?}",
            journal.events
        );
    }
}

#[test]
fn every_crash_point_is_exercised() {
    // The quarantine run journals at most four entries (intent, move, verify,
    // retain), so limits 0..=4 cover a crash before each and none at all.
    let world = World::new(None);
    let mut journal = Journal::crashing_after(usize::MAX, operation());
    let outcome = quarantine_item(&world, &mut journal, &Free, &confirmed());
    assert!(outcome.is_ok());
    assert_eq!(journal.events.len(), 4);
}
