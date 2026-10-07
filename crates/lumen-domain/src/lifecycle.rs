//! Scan and cleanup-plan lifecycles.
//!
//! ```text
//! Scan:  Created → Running ⇄ Paused → Completed | Cancelled | Failed
//! Plan:  Draft → Proposed → Confirmed → Executing → Completed | PartiallyCompleted
//!          └────────┴─────→ Abandoned                     └──────→ RolledBack
//! ```
//!
//! A plan executes only after the user confirmed *that exact plan*: the
//! confirmation names the plan's hash, and a mismatch is rejected (threat model
//! TB2/TB3: the UI must not be able to confirm one plan and execute another).

use serde::{Deserialize, Serialize};

use crate::{PlanHash, Timestamp};

/// Lifecycle state of a scan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScanState {
    /// Created, not started.
    Created,
    /// Enumerating.
    Running,
    /// Paused; resumable from its checkpoint.
    Paused,
    /// Finished.
    Completed,
    /// Stopped by the user; partial results are kept and marked as partial.
    Cancelled,
    /// Stopped by an error; partial results are kept and marked as partial.
    Failed,
}

/// Event reported by the scan engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScanEvent {
    /// Start enumerating.
    Start,
    /// Pause at the next checkpoint.
    Pause,
    /// Resume from the checkpoint.
    Resume,
    /// Enumeration finished.
    Complete,
    /// The user cancelled.
    Cancel,
    /// An unrecoverable error stopped the scan.
    Fail,
}

impl ScanState {
    /// Applies an event.
    ///
    /// # Errors
    ///
    /// Returns [`ScanTransitionError`] for a transition the lifecycle forbids.
    pub fn apply(self, event: ScanEvent) -> Result<Self, ScanTransitionError> {
        use ScanEvent as E;
        use ScanState as S;
        Ok(match (self, event) {
            (S::Created, E::Start) | (S::Paused, E::Resume) => S::Running,
            (S::Running, E::Pause) => S::Paused,
            (S::Running, E::Complete) => S::Completed,
            (S::Created | S::Running | S::Paused, E::Cancel) => S::Cancelled,
            (S::Running, E::Fail) => S::Failed,
            _ => return Err(ScanTransitionError { from: self, event }),
        })
    }

    /// Whether the scan has ended.
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Cancelled | Self::Failed)
    }

    /// Whether results exist but do not cover everything the scan intended.
    pub const fn results_are_partial(self) -> bool {
        matches!(self, Self::Cancelled | Self::Failed)
    }
}

/// Error returned for a forbidden scan transition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("scan transition {event:?} is not allowed from {from:?}")]
pub struct ScanTransitionError {
    /// State the scan was in.
    pub from: ScanState,
    /// Rejected event.
    pub event: ScanEvent,
}

/// Where a confirmation was given.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfirmationSurface {
    /// The desktop app.
    DesktopApp,
    /// The optional local browser dashboard.
    BrowserDashboard,
    /// The mobile app.
    MobileApp,
}

/// A user's confirmation of one specific plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Confirmation {
    /// Hash of the plan the user saw and confirmed.
    pub plan_hash: PlanHash,
    /// When the user confirmed.
    pub confirmed_at: Timestamp,
    /// Where the user confirmed.
    pub surface: ConfirmationSurface,
}

/// Lifecycle state of a cleanup plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum PlanState {
    /// Being assembled.
    Draft,
    /// Shown to the user, identified by its hash.
    Proposed {
        /// Hash of the proposed plan.
        plan_hash: PlanHash,
    },
    /// The user confirmed this exact plan.
    Confirmed {
        /// The confirmation, naming the plan hash.
        confirmation: Confirmation,
    },
    /// Items are being executed.
    Executing {
        /// The confirmation that authorized execution.
        confirmation: Confirmation,
    },
    /// Every item succeeded.
    Completed,
    /// Some items were skipped or failed (each item's own lifecycle says which).
    PartiallyCompleted,
    /// Completed items were restored.
    RolledBack,
    /// Discarded before execution.
    Abandoned,
}

/// Event in a plan's lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum PlanEvent {
    /// Present the plan to the user.
    Propose {
        /// Hash of the plan's canonical form.
        plan_hash: PlanHash,
    },
    /// The user confirmed.
    Confirm {
        /// The confirmation.
        confirmation: Confirmation,
    },
    /// Execution started.
    Start,
    /// Execution finished.
    Finish {
        /// Whether every item succeeded.
        all_succeeded: bool,
    },
    /// Restore everything that was executed.
    RollBack,
    /// Discard before execution.
    Abandon,
}

/// Error returned for a forbidden plan transition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum PlanTransitionError {
    /// The lifecycle does not allow this event here.
    #[error("plan transition {event:?} is not allowed from {from:?}")]
    NotAllowed {
        /// State the plan was in.
        from: PlanState,
        /// Rejected event.
        event: PlanEvent,
    },
    /// The confirmation names a different plan than the one proposed.
    #[error("confirmation is for plan {confirmed} but plan {proposed} was proposed")]
    HashMismatch {
        /// Hash of the proposed plan.
        proposed: PlanHash,
        /// Hash in the confirmation.
        confirmed: PlanHash,
    },
}

impl PlanState {
    /// Applies an event.
    ///
    /// # Errors
    ///
    /// Returns [`PlanTransitionError::HashMismatch`] if a confirmation names a
    /// different plan, and [`PlanTransitionError::NotAllowed`] for any other
    /// forbidden transition.
    pub fn apply(self, event: PlanEvent) -> Result<Self, PlanTransitionError> {
        use PlanEvent as E;
        use PlanState as S;
        Ok(match (self, event) {
            (S::Draft, E::Propose { plan_hash }) => S::Proposed { plan_hash },
            (S::Proposed { plan_hash }, E::Confirm { confirmation }) => {
                if confirmation.plan_hash != plan_hash {
                    return Err(PlanTransitionError::HashMismatch {
                        proposed: plan_hash,
                        confirmed: confirmation.plan_hash,
                    });
                }
                S::Confirmed { confirmation }
            }
            (S::Confirmed { confirmation }, E::Start) => S::Executing { confirmation },
            (
                S::Executing { .. },
                E::Finish {
                    all_succeeded: true,
                },
            ) => S::Completed,
            (
                S::Executing { .. },
                E::Finish {
                    all_succeeded: false,
                },
            ) => S::PartiallyCompleted,
            (S::Completed | S::PartiallyCompleted, E::RollBack) => S::RolledBack,
            (S::Draft | S::Proposed { .. } | S::Confirmed { .. }, E::Abandon) => S::Abandoned,
            _ => return Err(PlanTransitionError::NotAllowed { from: self, event }),
        })
    }

    /// The confirmation authorizing the plan, once confirmed.
    pub const fn confirmation(&self) -> Option<&Confirmation> {
        match self {
            Self::Confirmed { confirmation } | Self::Executing { confirmation } => {
                Some(confirmation)
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn hash(n: u8) -> PlanHash {
        PlanHash::from_bytes([n; 32])
    }

    fn confirmation(n: u8) -> Confirmation {
        Confirmation {
            plan_hash: hash(n),
            confirmed_at: Timestamp::UNIX_EPOCH,
            surface: ConfirmationSurface::DesktopApp,
        }
    }

    #[test]
    fn scan_lifecycle() -> Result<(), ScanTransitionError> {
        use ScanEvent as E;
        let s = ScanState::Created
            .apply(E::Start)?
            .apply(E::Pause)?
            .apply(E::Resume)?;
        assert_eq!(s, ScanState::Running);
        assert!(s.apply(E::Complete)?.is_terminal());
        assert!(s.apply(E::Cancel)?.results_are_partial());
        assert!(ScanState::Completed.apply(E::Resume).is_err());
        assert!(
            ScanState::Paused.apply(E::Complete).is_err(),
            "complete only while running"
        );
        Ok(())
    }

    #[test]
    fn plan_executes_only_after_confirming_the_proposed_hash() -> Result<(), PlanTransitionError> {
        use PlanEvent as E;
        let proposed = PlanState::Draft.apply(E::Propose { plan_hash: hash(1) })?;
        assert!(
            proposed.apply(E::Start).is_err(),
            "cannot execute an unconfirmed plan"
        );
        assert_eq!(
            proposed.apply(E::Confirm {
                confirmation: confirmation(2)
            }),
            Err(PlanTransitionError::HashMismatch {
                proposed: hash(1),
                confirmed: hash(2)
            })
        );
        let executing = proposed
            .apply(E::Confirm {
                confirmation: confirmation(1),
            })?
            .apply(E::Start)?;
        assert_eq!(executing.confirmation().map(|c| c.plan_hash), Some(hash(1)));
        assert_eq!(
            executing.apply(E::Finish {
                all_succeeded: false
            })?,
            PlanState::PartiallyCompleted
        );
        assert!(
            executing.apply(E::Abandon).is_err(),
            "cannot abandon mid-execution"
        );
        Ok(())
    }

    #[test]
    fn rollback_only_after_execution() -> Result<(), PlanTransitionError> {
        use PlanEvent as E;
        assert!(PlanState::Draft.apply(E::RollBack).is_err());
        assert_eq!(
            PlanState::Completed.apply(E::RollBack)?,
            PlanState::RolledBack
        );
        Ok(())
    }

    fn arb_plan_event() -> impl Strategy<Value = PlanEvent> {
        use PlanEvent as E;
        prop_oneof![
            (0u8..3).prop_map(|n| E::Propose { plan_hash: hash(n) }),
            (0u8..3).prop_map(|n| E::Confirm {
                confirmation: confirmation(n)
            }),
            Just(E::Start),
            any::<bool>().prop_map(|all_succeeded| E::Finish { all_succeeded }),
            Just(E::RollBack),
            Just(E::Abandon),
        ]
    }

    proptest! {
        #[test]
        fn execution_always_carries_a_confirmation_for_the_proposed_plan(
            events in prop::collection::vec(arb_plan_event(), 0..16),
        ) {
            let mut state = PlanState::Draft;
            let mut proposed = None;
            for event in events {
                let Ok(next) = state.apply(event) else { continue };
                if let PlanState::Proposed { plan_hash } = next {
                    proposed = Some(plan_hash);
                }
                if let PlanState::Executing { confirmation } = next {
                    prop_assert_eq!(Some(confirmation.plan_hash), proposed);
                }
                state = next;
            }
        }
    }
}
