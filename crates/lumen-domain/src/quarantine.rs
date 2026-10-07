//! The quarantine item lifecycle (ADR-0015).
//!
//! ```text
//! Planned → Journaled → Moved → Verified → Retained → Restored | Finalized
//!    └──────── Failed{at source} ───────┘     │
//!                     Moved/Verified → Failed{in quarantine} → Restored
//! ```
//!
//! Executors drive this machine; it only permits transitions that keep the item
//! either untouched at its source or restorable from quarantine. Permanent
//! deletion ([`QuarantineState::Finalized`]) is reachable only after retention.

use serde::{Deserialize, Serialize};

/// Why a quarantine step failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum QuarantineFailure {
    /// The item's identity no longer matched the plan (changed or swapped).
    IdentityMismatch,
    /// A process was using the item.
    InUse,
    /// The volume cannot quarantine safely (no no-replace rename, read-only, …).
    VolumeUnsupported,
    /// The move itself failed.
    MoveFailed,
    /// Post-move verification did not match the plan.
    VerificationFailed,
}

/// Where the item physically is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemLocation {
    /// At its original location.
    Source,
    /// Inside Lumen's quarantine store.
    Quarantine,
    /// Permanently removed (only after finalization).
    Gone,
}

/// Lifecycle state of one quarantined item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum QuarantineState {
    /// Part of a confirmed plan; nothing done yet.
    Planned,
    /// Intent durably written to the ledger; the move may now happen.
    Journaled,
    /// Renamed into the quarantine store; not yet verified.
    Moved,
    /// Verified in quarantine (identity, link count and size facts match).
    Verified,
    /// Held for the retention period; restorable.
    Retained,
    /// Moved back to its original location. Terminal.
    Restored,
    /// Permanently removed after retention. Terminal.
    Finalized,
    /// A step failed; `at` says where the item is.
    Failed {
        /// Why it failed.
        reason: QuarantineFailure,
        /// Where the item is: untouched at its source, or in quarantine and
        /// restorable.
        at: ItemLocation,
    },
}

/// An event an executor reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum QuarantineEvent {
    /// The intent record is durable.
    Journal,
    /// The rename into quarantine succeeded.
    Move,
    /// Verification in quarantine succeeded.
    Verify,
    /// Retention began.
    Retain,
    /// The item was moved back.
    Restore,
    /// The item was permanently removed.
    Finalize,
    /// A step failed.
    Fail {
        /// Why.
        reason: QuarantineFailure,
    },
}

/// Error returned for a transition the lifecycle does not allow.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("quarantine transition {event:?} is not allowed from {from:?}")]
pub struct TransitionError {
    /// State the item was in.
    pub from: QuarantineState,
    /// Rejected event.
    pub event: QuarantineEvent,
}

impl QuarantineState {
    /// Applies an event, returning the next state or an error if the lifecycle
    /// forbids it.
    ///
    /// # Errors
    ///
    /// Returns [`TransitionError`] for any transition not in the lifecycle.
    pub fn apply(self, event: QuarantineEvent) -> Result<Self, TransitionError> {
        use QuarantineEvent as E;
        use QuarantineState as S;
        let next = match (self, event) {
            (S::Planned, E::Journal) => S::Journaled,
            (S::Journaled, E::Move) => S::Moved,
            (S::Moved, E::Verify) => S::Verified,
            (S::Verified, E::Retain) => S::Retained,
            // Restore after retention, or roll back an item that failed after the move.
            (
                S::Retained
                | S::Failed {
                    at: ItemLocation::Quarantine,
                    ..
                },
                E::Restore,
            ) => S::Restored,
            (S::Retained, E::Finalize) => S::Finalized,
            (S::Planned | S::Journaled, E::Fail { reason }) => S::Failed {
                reason,
                at: ItemLocation::Source,
            },
            (S::Moved | S::Verified, E::Fail { reason }) => S::Failed {
                reason,
                at: ItemLocation::Quarantine,
            },
            _ => return Err(TransitionError { from: self, event }),
        };
        Ok(next)
    }

    /// Where the item physically is in this state.
    pub const fn item_location(self) -> ItemLocation {
        match self {
            Self::Planned | Self::Journaled | Self::Restored => ItemLocation::Source,
            Self::Moved | Self::Verified | Self::Retained => ItemLocation::Quarantine,
            Self::Finalized => ItemLocation::Gone,
            Self::Failed { at, .. } => at,
        }
    }

    /// Whether no further transition is possible.
    pub const fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Restored
                | Self::Finalized
                | Self::Failed {
                    at: ItemLocation::Source | ItemLocation::Gone,
                    ..
                }
        )
    }

    /// Whether the item can still be restored to its original location.
    pub const fn is_restorable(self) -> bool {
        matches!(
            self,
            Self::Retained
                | Self::Failed {
                    at: ItemLocation::Quarantine,
                    ..
                }
        )
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn run(events: &[QuarantineEvent]) -> Result<QuarantineState, TransitionError> {
        events
            .iter()
            .try_fold(QuarantineState::Planned, |state, e| state.apply(*e))
    }

    #[test]
    fn happy_path_and_restore() -> Result<(), TransitionError> {
        use QuarantineEvent as E;
        let retained = run(&[E::Journal, E::Move, E::Verify, E::Retain])?;
        assert_eq!(retained, QuarantineState::Retained);
        assert!(retained.is_restorable());
        assert_eq!(retained.apply(E::Restore)?, QuarantineState::Restored);
        assert_eq!(
            retained.apply(E::Finalize)?.item_location(),
            ItemLocation::Gone
        );
        Ok(())
    }

    #[test]
    fn cannot_move_without_journal_or_finalize_without_retention() {
        use QuarantineEvent as E;
        assert!(QuarantineState::Planned.apply(E::Move).is_err());
        assert!(run(&[E::Journal, E::Move, E::Verify, E::Finalize]).is_err());
        assert!(run(&[E::Journal, E::Move, E::Finalize]).is_err());
    }

    #[test]
    fn failure_after_move_is_restorable() -> Result<(), TransitionError> {
        use QuarantineEvent as E;
        let failed = run(&[
            E::Journal,
            E::Move,
            E::Fail {
                reason: QuarantineFailure::VerificationFailed,
            },
        ])?;
        assert_eq!(failed.item_location(), ItemLocation::Quarantine);
        assert!(failed.is_restorable() && !failed.is_terminal());
        assert_eq!(failed.apply(E::Restore)?, QuarantineState::Restored);
        assert!(
            failed.apply(E::Finalize).is_err(),
            "a failed item is never finalized"
        );
        Ok(())
    }

    #[test]
    fn failure_before_move_leaves_item_at_source() -> Result<(), TransitionError> {
        let failed = QuarantineState::Journaled.apply(QuarantineEvent::Fail {
            reason: QuarantineFailure::InUse,
        })?;
        assert_eq!(failed.item_location(), ItemLocation::Source);
        assert!(failed.is_terminal());
        Ok(())
    }

    #[test]
    fn json_shape() -> serde_json::Result<()> {
        let s = QuarantineState::Failed {
            reason: QuarantineFailure::IdentityMismatch,
            at: ItemLocation::Source,
        };
        assert_eq!(
            serde_json::to_string(&s)?,
            r#"{"state":"failed","reason":"identity_mismatch","at":"source"}"#
        );
        assert_eq!(
            serde_json::from_str::<QuarantineState>(&serde_json::to_string(&s)?)?,
            s
        );
        Ok(())
    }

    fn arb_event() -> impl Strategy<Value = QuarantineEvent> {
        use QuarantineEvent as E;
        prop_oneof![
            Just(E::Journal),
            Just(E::Move),
            Just(E::Verify),
            Just(E::Retain),
            Just(E::Restore),
            Just(E::Finalize),
            Just(E::Fail {
                reason: QuarantineFailure::MoveFailed
            }),
            Just(E::Fail {
                reason: QuarantineFailure::IdentityMismatch
            }),
        ]
    }

    proptest! {
        #[test]
        fn lifecycle_invariants_hold_for_any_event_sequence(events in prop::collection::vec(arb_event(), 0..20)) {
            use QuarantineEvent as E;
            let mut state = QuarantineState::Planned;
            let mut journaled = false;
            let mut retained = false;
            for event in events {
                let Ok(next) = state.apply(event) else {
                    continue; // Rejected events leave the state unchanged.
                };
                journaled |= next == QuarantineState::Journaled;
                retained |= next == QuarantineState::Retained;
                if next.item_location() == ItemLocation::Quarantine {
                    prop_assert!(journaled, "reached quarantine without a journal intent");
                }
                if next == QuarantineState::Finalized {
                    prop_assert!(retained, "finalized without retention");
                }
                // Physical location changes only through Move, Restore, Finalize.
                let (before, after) = (state.item_location(), next.item_location());
                if before != after {
                    let allowed = matches!(
                        (before, after, event),
                        (ItemLocation::Source, ItemLocation::Quarantine, E::Move)
                            | (ItemLocation::Quarantine, ItemLocation::Source, E::Restore)
                            | (ItemLocation::Quarantine, ItemLocation::Gone, E::Finalize)
                    );
                    prop_assert!(allowed, "{before:?} -> {after:?} via {event:?}");
                }
                prop_assert!(!state.is_terminal(), "terminal state {state:?} accepted {event:?}");
                state = next;
            }
        }
    }
}
