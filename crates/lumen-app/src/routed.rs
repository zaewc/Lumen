//! Items Lumen's helper holds, routed to the helper (ADR-0063 rule 6, roadmap
//! 16.3d).
//!
//! Lumen keeps the journal and the ledger for every item, the helper's too.
//! What the journal records for an item is the executor's record
//! ([`PreparedItem`]), opaque to everything but the executor. For an item the
//! helper moves, Lumen records a [`HelperItem`] instead: the planned path,
//! identity and evidence hash the helper is asked about, and Lumen's own
//! executor record of the item (for its original path). No journal schema
//! changes.
//!
//! [`Routed`] is the executor every held-item flow uses. It sends a
//! [`HelperItem`] to the helper and anything else to Lumen's own executor:
//!
//! - `quarantine` asks the helper's `quarantine` verb;
//! - `locate`, `restore` and `finalize` ask its verbs by operation;
//! - the helper checks every process itself before it finalizes (only a
//!   complete "not in use" passes), so [`QuarantineExecutor::checks_use_itself`]
//!   is true and Lumen's own probe, which cannot see into the helper's
//!   root-owned store, is not asked;
//! - the helper keeps its record after a restore (owner, 2026-11-16), so
//!   `forget` removes nothing.
//!
//! An answer that is not one the verb gives, or no answer, never counts as
//! success. For `quarantine`, only the helper's own refusal or failure says
//! where the item is; otherwise the helper is asked where it is, and an item
//! it may hold is reported in the store, failed, so Lumen keeps track of it
//! and the person can put it back. It is never reported at its source unless
//! the helper says so. A restore that fails or goes unanswered is checked the
//! same way: only the helper's "at its source" reports it put back. A
//! finalize that goes unanswered may have removed part of the item, so it is
//! never reported intact; only the helper's own refusal says nothing was
//! removed.

use std::path::PathBuf;

use lumen_application::helper::{
    FinalizeRefusal, HeldRequest, HelperRequest, HelperResponse, PROTOCOL, QuarantineRequest,
    Whereabouts,
};
use lumen_application::ports::{
    ExecutionFailure, Located, NotRestored, PreparedItem, QuarantineExecutor, Removal,
    RestoreFailure, RestoredItem,
};
use lumen_domain::{
    EvidenceHash, FileIdentity, ItemLocation, OperationId, QuarantineFailure, RawPath,
    UnixOwnership,
};
use serde::{Deserialize, Serialize};

/// What the journal records for an item the helper moves.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HelperItem {
    /// The planned item's path.
    pub path: RawPath,
    /// Its identity when the person confirmed it.
    pub identity: FileIdentity,
    /// The confirmed decision's evidence hash.
    pub evidence_hash: EvidenceHash,
    /// Lumen's own executor record of it, taken without privileges.
    pub executor: serde_json::Value,
}

/// The journal's form: `{"helper": {...}}`, which no executor record has.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Wrapped {
    helper: HelperItem,
}

impl HelperItem {
    /// The record to journal for this item; `None` if it cannot be written.
    #[must_use]
    pub fn prepared(&self) -> Option<PreparedItem> {
        serde_json::to_vec(&Wrapped {
            helper: self.clone(),
        })
        .ok()
        .map(PreparedItem)
    }

    /// The helper item `item` records, or `None` for any other record.
    #[must_use]
    pub fn of(item: &PreparedItem) -> Option<Self> {
        serde_json::from_slice::<Wrapped>(&item.0)
            .ok()
            .map(|w| w.helper)
    }
}

/// Where the item `item` records was, whoever holds it.
#[must_use]
pub fn original_path(item: &PreparedItem) -> Option<PathBuf> {
    match HelperItem::of(item) {
        Some(helper) => serde_json::to_vec(&helper.executor)
            .ok()
            .and_then(|bytes| lumen_fs_exec::original_path(&PreparedItem(bytes))),
        None => lumen_fs_exec::original_path(item),
    }
}

/// The connection to Lumen's helper.
pub trait HelperLink: Send + Sync {
    /// The helper's answer to `request`, or `None` if it could not be asked
    /// or did not answer with something Lumen reads.
    fn ask(&self, request: &HelperRequest) -> Option<HelperResponse>;
}

/// No helper: every question goes unanswered (off macOS, and in tests).
#[derive(Debug, Clone, Copy, Default)]
pub struct NoHelper;

impl HelperLink for NoHelper {
    fn ask(&self, _request: &HelperRequest) -> Option<HelperResponse> {
        None
    }
}

/// Lumen's own executor, with the helper's items routed to the helper.
pub struct Routed<'a, E> {
    own: E,
    helper: &'a dyn HelperLink,
}

impl<E: std::fmt::Debug> std::fmt::Debug for Routed<'_, E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Routed")
            .field("own", &self.own)
            .finish_non_exhaustive()
    }
}

impl<'a, E: QuarantineExecutor> Routed<'a, E> {
    /// Lumen's executor `own`, and the helper behind `helper`.
    pub fn new(own: E, helper: &'a dyn HelperLink) -> Self {
        Self { own, helper }
    }

    fn ask_held(
        &self,
        verb: fn(HeldRequest) -> HelperRequest,
        operation: OperationId,
    ) -> Option<HelperResponse> {
        self.helper.ask(&verb(HeldRequest {
            protocol: PROTOCOL,
            operation,
        }))
    }
}

/// Nothing moved.
const AT_SOURCE: ExecutionFailure = ExecutionFailure {
    reason: QuarantineFailure::MoveFailed,
    left: ItemLocation::Source,
};

/// The helper's answer to `quarantine` was lost or unexpected, and it may
/// hold the item: kept in the store, failed, so restore can put it back.
const MAYBE_HELD: ExecutionFailure = ExecutionFailure {
    reason: QuarantineFailure::VerificationFailed,
    left: ItemLocation::Quarantine,
};

impl<E: QuarantineExecutor> QuarantineExecutor for Routed<'_, E> {
    fn prepare(
        &self,
        root: &RawPath,
        relative: &[Vec<u8>],
    ) -> Result<PreparedItem, ExecutionFailure> {
        self.own.prepare(root, relative)
    }

    fn quarantine(
        &self,
        item: &PreparedItem,
        operation: OperationId,
        manifest: &[u8],
    ) -> Result<(), ExecutionFailure> {
        let Some(helper) = HelperItem::of(item) else {
            return self.own.quarantine(item, operation, manifest);
        };
        // The helper writes its own record; Lumen's manifest is the journal's.
        let request = HelperRequest::Quarantine(QuarantineRequest {
            protocol: PROTOCOL,
            operation,
            path: helper.path,
            identity: helper.identity,
            evidence_hash: helper.evidence_hash,
        });
        match self.helper.ask(&request) {
            Some(HelperResponse::Quarantined { operation: moved }) if moved == operation => Ok(()),
            Some(HelperResponse::Failed { reason, left }) => Err(ExecutionFailure { reason, left }),
            Some(HelperResponse::Ineligible { .. }) => Err(ExecutionFailure {
                reason: QuarantineFailure::IdentityMismatch,
                left: ItemLocation::Source,
            }),
            // Refused before it looked: the helper moved nothing.
            Some(HelperResponse::Refused { .. }) => Err(AT_SOURCE),
            // Unanswered or unexpected: the helper may have moved it, so ask
            // where it is. Only "at its source" means nothing moved.
            _ => match self.ask_held(HelperRequest::Locate, operation) {
                Some(HelperResponse::Located {
                    whereabouts: Whereabouts::AtSource,
                }) => Err(AT_SOURCE),
                _ => Err(MAYBE_HELD),
            },
        }
    }

    fn locate(&self, item: &PreparedItem, operation: OperationId) -> Located {
        if HelperItem::of(item).is_none() {
            return self.own.locate(item, operation);
        }
        match self.ask_held(HelperRequest::Locate, operation) {
            Some(HelperResponse::Located { whereabouts }) => whereabouts.into(),
            _ => Located::Unknown,
        }
    }

    fn restore(
        &self,
        item: &PreparedItem,
        operation: OperationId,
    ) -> Result<RestoredItem, RestoreFailure> {
        if HelperItem::of(item).is_none() {
            return self.own.restore(item, operation);
        }
        let mut failure = match self.ask_held(HelperRequest::Restore, operation) {
            Some(HelperResponse::Restored {
                parent_replaced,
                matches_record,
            }) => {
                return Ok(RestoredItem {
                    parent_replaced,
                    matches_record,
                });
            }
            Some(HelperResponse::NotRestored { reason, left }) => RestoreFailure {
                reason: reason.into(),
                left,
            },
            _ => RestoreFailure {
                reason: NotRestored::Failed,
                left: ItemLocation::Quarantine,
            },
        };
        // A lost answer may follow a completed restore, including one from
        // a previous process. Locate after refusal too: a retry can find no
        // held item because that earlier restore already put it back.
        if failure.left == ItemLocation::Quarantine
            && matches!(
                self.ask_held(HelperRequest::Locate, operation),
                Some(HelperResponse::Located {
                    whereabouts: Whereabouts::AtSource
                })
            )
        {
            failure.left = ItemLocation::Source;
        }
        Err(failure)
    }

    fn finalize(&self, item: &PreparedItem, operation: OperationId) -> Result<(), Removal> {
        if HelperItem::of(item).is_none() {
            return self.own.finalize(item, operation);
        }
        match self.ask_held(HelperRequest::Finalize, operation) {
            Some(HelperResponse::Finalized) => Ok(()),
            Some(HelperResponse::NotFinalized {
                reason: FinalizeRefusal::FailedPartly,
            }) => Err(Removal::Partly),
            // Explicit refusal establishes that nothing was removed.
            Some(
                HelperResponse::NotFinalized { .. }
                | HelperResponse::Refused { .. }
                | HelperResponse::NotHeld,
            ) => Err(Removal::Nothing),
            // A lost answer may follow partial removal. Never claim the
            // held item is intact when its whereabouts are unknown.
            _ => Err(Removal::Partly),
        }
    }

    fn checks_use_itself(&self, item: &PreparedItem) -> bool {
        HelperItem::of(item).is_some() || self.own.checks_use_itself(item)
    }

    fn unix_ownership(&self, item: &PreparedItem) -> Option<UnixOwnership> {
        HelperItem::of(item)
            .is_none()
            .then(|| self.own.unix_ownership(item))
            .flatten()
    }

    fn forget(&self, item: &PreparedItem, operation: OperationId) -> bool {
        // The helper keeps its record after a restore (owner, 2026-11-16).
        HelperItem::of(item).is_none() && self.own.forget(item, operation)
    }

    fn held(&self, item: &PreparedItem, operation: OperationId) -> Option<(RawPath, FileIdentity)> {
        // Inside the helper's root-owned store, which Lumen cannot see into.
        HelperItem::of(item)
            .is_none()
            .then(|| self.own.held(item, operation))
            .flatten()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use lumen_application::helper::{HelperRefusal, RestoreRefusal};
    use lumen_domain::{FileId, VolumeId};

    use super::*;

    const OPERATION: &str = "0190f1c2-7a3b-7c4d-8e5f-6a7b8c9d0e1f";

    fn operation() -> OperationId {
        OPERATION.parse().unwrap_or_else(|_| unreachable!())
    }

    fn helper_item() -> HelperItem {
        HelperItem {
            path: RawPath::from_unix_bytes(b"/Applications/Word.app".to_vec())
                .unwrap_or_else(|_| unreachable!()),
            identity: FileIdentity {
                volume: VolumeId::new("dev:1").unwrap_or_else(|_| unreachable!()),
                file: FileId::new(7),
            },
            evidence_hash: EvidenceHash::from_bytes([3; 32]),
            executor: serde_json::json!({"leaf": [87]}),
        }
    }

    /// Lumen's own executor: records what it was asked.
    #[derive(Default)]
    struct Own(Mutex<Vec<&'static str>>);

    impl Own {
        fn note(&self, what: &'static str) {
            self.0
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push(what);
        }

        fn asked(&self) -> Vec<&'static str> {
            self.0
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .clone()
        }
    }

    impl QuarantineExecutor for &Own {
        fn prepare(&self, _: &RawPath, _: &[Vec<u8>]) -> Result<PreparedItem, ExecutionFailure> {
            self.note("prepare");
            Err(AT_SOURCE)
        }

        fn quarantine(
            &self,
            _: &PreparedItem,
            _: OperationId,
            _: &[u8],
        ) -> Result<(), ExecutionFailure> {
            self.note("quarantine");
            Ok(())
        }

        fn locate(&self, _: &PreparedItem, _: OperationId) -> Located {
            self.note("locate");
            Located::InStore
        }

        fn restore(
            &self,
            _: &PreparedItem,
            _: OperationId,
        ) -> Result<RestoredItem, RestoreFailure> {
            self.note("restore");
            Ok(RestoredItem {
                parent_replaced: false,
                matches_record: true,
            })
        }

        fn finalize(&self, _: &PreparedItem, _: OperationId) -> Result<(), Removal> {
            self.note("finalize");
            Ok(())
        }

        fn forget(&self, _: &PreparedItem, _: OperationId) -> bool {
            self.note("forget");
            true
        }
    }

    /// The helper: answers with `answer`, records what it was asked.
    struct Helper {
        answer: Option<HelperResponse>,
        asked: Mutex<Vec<HelperRequest>>,
    }

    impl Helper {
        fn answering(answer: Option<HelperResponse>) -> Self {
            Self {
                answer,
                asked: Mutex::new(Vec::new()),
            }
        }

        fn asked(&self) -> Vec<HelperRequest> {
            self.asked
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .clone()
        }
    }

    impl HelperLink for Helper {
        fn ask(&self, request: &HelperRequest) -> Option<HelperResponse> {
            self.asked
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push(request.clone());
            self.answer.clone()
        }
    }

    /// A helper that gives `answers` in turn, then none.
    struct Script(Mutex<std::collections::VecDeque<Option<HelperResponse>>>);

    impl Script {
        fn new<const N: usize>(answers: [Option<HelperResponse>; N]) -> Self {
            Self(Mutex::new(answers.into_iter().collect()))
        }
    }

    impl HelperLink for Script {
        fn ask(&self, _: &HelperRequest) -> Option<HelperResponse> {
            self.0
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .pop_front()
                .flatten()
        }
    }

    fn wrapped() -> PreparedItem {
        helper_item().prepared().unwrap_or_else(|| unreachable!())
    }

    #[test]
    fn a_helper_item_round_trips_and_names_its_original_path() {
        let item = wrapped();
        assert_eq!(HelperItem::of(&item), Some(helper_item()));
        assert_eq!(
            HelperItem::of(&PreparedItem(br#"{"leaf":[87]}"#.to_vec())),
            None
        );
        assert_eq!(HelperItem::of(&PreparedItem(b"not json".to_vec())), None);
        // The executor record here is not a real one, so there is no path;
        // a helper item never yields its own fields as a path.
        assert_eq!(original_path(&item), None);
    }

    #[test]
    fn other_items_go_to_lumens_own_executor() {
        let own = Own::default();
        let helper = Helper::answering(None);
        let routed = Routed::new(&own, &helper);
        let item = PreparedItem(br#"{"leaf":[87]}"#.to_vec());
        assert_eq!(routed.quarantine(&item, operation(), b"{}"), Ok(()));
        assert_eq!(routed.locate(&item, operation()), Located::InStore);
        assert!(routed.restore(&item, operation()).is_ok());
        assert_eq!(routed.finalize(&item, operation()), Ok(()));
        assert!(routed.forget(&item, operation()));
        assert!(!routed.checks_use_itself(&item));
        assert_eq!(
            own.asked(),
            ["quarantine", "locate", "restore", "finalize", "forget"]
        );
        assert_eq!(helper.asked(), Vec::<HelperRequest>::new());
    }

    #[test]
    fn a_helper_item_is_quarantined_by_the_helper_with_the_planned_facts() {
        let own = Own::default();
        let helper = Helper::answering(Some(HelperResponse::Quarantined {
            operation: operation(),
        }));
        let routed = Routed::new(&own, &helper);
        assert_eq!(routed.quarantine(&wrapped(), operation(), b"{}"), Ok(()));
        let item = helper_item();
        assert_eq!(
            helper.asked(),
            [HelperRequest::Quarantine(QuarantineRequest {
                protocol: PROTOCOL,
                operation: operation(),
                path: item.path,
                identity: item.identity,
                evidence_hash: item.evidence_hash,
            })]
        );
        assert_eq!(own.asked(), Vec::<&str>::new());
    }

    #[test]
    fn anything_but_the_helpers_success_is_not_success() {
        let other: OperationId = "0190f1c2-7a3b-7c4d-8e5f-6a7b8c9d0e2f"
            .parse()
            .unwrap_or_else(|_| unreachable!());
        let own = Own::default();
        // The helper says where the item is.
        for (answer, left) in [
            (
                HelperResponse::Refused {
                    reason: HelperRefusal::OtherProtocol,
                },
                ItemLocation::Source,
            ),
            (
                HelperResponse::Ineligible {
                    reasons: Vec::new(),
                },
                ItemLocation::Source,
            ),
            (
                HelperResponse::Failed {
                    reason: QuarantineFailure::VerificationFailed,
                    left: ItemLocation::Quarantine,
                },
                ItemLocation::Quarantine,
            ),
        ] {
            let helper = Helper::answering(Some(answer.clone()));
            let routed = Routed::new(&own, &helper);
            let failed = routed.quarantine(&wrapped(), operation(), b"{}");
            assert_eq!(failed.map_err(|f| f.left), Err(left), "{answer:?}");
            assert_eq!(helper.asked().len(), 1, "{answer:?}");
        }
        // No answer, or one `quarantine` never gives: the helper may hold it.
        for answer in [
            None,
            Some(HelperResponse::NotHeld),
            Some(HelperResponse::Quarantined { operation: other }),
        ] {
            let helper = Helper::answering(answer.clone());
            let routed = Routed::new(&own, &helper);
            let failed = routed.quarantine(&wrapped(), operation(), b"{}");
            assert_eq!(failed, Err(MAYBE_HELD), "{answer:?}");
            assert!(
                matches!(helper.asked().as_slice(), [_, HelperRequest::Locate(_)]),
                "{answer:?}"
            );
        }
        // Only the helper's own "at its source" means nothing moved.
        let at_source = Script::new([
            None,
            Some(HelperResponse::Located {
                whereabouts: Whereabouts::AtSource,
            }),
        ]);
        let routed = Routed::new(&own, &at_source);
        assert_eq!(
            routed.quarantine(&wrapped(), operation(), b"{}"),
            Err(AT_SOURCE)
        );
        let in_store = Script::new([
            None,
            Some(HelperResponse::Located {
                whereabouts: Whereabouts::InStore,
            }),
        ]);
        let routed = Routed::new(&own, &in_store);
        assert_eq!(
            routed.quarantine(&wrapped(), operation(), b"{}"),
            Err(MAYBE_HELD)
        );
        let helper = Helper::answering(None);
        let routed = Routed::new(&own, &helper);
        assert_eq!(routed.locate(&wrapped(), operation()), Located::Unknown);
        assert_eq!(
            routed.restore(&wrapped(), operation()),
            Err(RestoreFailure {
                reason: NotRestored::Failed,
                left: ItemLocation::Quarantine
            })
        );
        assert_eq!(
            routed.finalize(&wrapped(), operation()),
            Err(Removal::Partly)
        );
        assert_eq!(own.asked(), Vec::<&str>::new());
    }

    #[test]
    fn a_lost_restore_answer_is_reconciled_without_inventing_success() {
        let own = Own::default();
        for (whereabouts, left) in [
            (Whereabouts::AtSource, ItemLocation::Source),
            (Whereabouts::InStore, ItemLocation::Quarantine),
            (Whereabouts::Unknown, ItemLocation::Quarantine),
        ] {
            let helper = Script::new([None, Some(HelperResponse::Located { whereabouts })]);
            assert_eq!(
                Routed::new(&own, &helper).restore(&wrapped(), operation()),
                Err(RestoreFailure {
                    reason: NotRestored::Failed,
                    left
                })
            );
        }
        let retry = Script::new([
            Some(HelperResponse::NotRestored {
                reason: RestoreRefusal::ItemChanged,
                left: ItemLocation::Quarantine,
            }),
            Some(HelperResponse::Located {
                whereabouts: Whereabouts::AtSource,
            }),
        ]);
        assert_eq!(
            Routed::new(&own, &retry).restore(&wrapped(), operation()),
            Err(RestoreFailure {
                reason: NotRestored::ItemChanged,
                left: ItemLocation::Source
            })
        );
        for answer in [
            None,
            Some(HelperResponse::Located {
                whereabouts: Whereabouts::InStore,
            }),
            Some(HelperResponse::NotHeld),
        ] {
            let helper = Helper::answering(answer);
            assert_eq!(
                Routed::new(&own, &helper).finalize(&wrapped(), operation()),
                Err(if matches!(helper.answer, Some(HelperResponse::NotHeld)) {
                    Removal::Nothing
                } else {
                    Removal::Partly
                })
            );
        }
        assert_eq!(own.asked(), Vec::<&str>::new());
    }

    #[test]
    fn held_verbs_ask_the_helper_by_operation() {
        let own = Own::default();
        let ask = |answer| Helper::answering(Some(answer));
        let located = ask(HelperResponse::Located {
            whereabouts: Whereabouts::AtSource,
        });
        assert_eq!(
            Routed::new(&own, &located).locate(&wrapped(), operation()),
            Located::AtSource
        );
        assert_eq!(
            located.asked(),
            [HelperRequest::Locate(HeldRequest {
                protocol: PROTOCOL,
                operation: operation()
            })]
        );
        let refused = ask(HelperResponse::NotRestored {
            reason: RestoreRefusal::NameTaken,
            left: ItemLocation::Quarantine,
        });
        assert_eq!(
            Routed::new(&own, &refused).restore(&wrapped(), operation()),
            Err(RestoreFailure {
                reason: NotRestored::NameTaken,
                left: ItemLocation::Quarantine
            })
        );
        for (answer, result) in [
            (HelperResponse::Finalized, Ok(())),
            (
                HelperResponse::NotFinalized {
                    reason: FinalizeRefusal::InUse,
                },
                Err(Removal::Nothing),
            ),
            (
                HelperResponse::NotFinalized {
                    reason: FinalizeRefusal::FailedPartly,
                },
                Err(Removal::Partly),
            ),
        ] {
            let helper = ask(answer);
            assert_eq!(
                Routed::new(&own, &helper).finalize(&wrapped(), operation()),
                result
            );
        }
        let routed = Routed::new(&own, &located);
        assert!(routed.checks_use_itself(&wrapped()));
        assert!(!routed.forget(&wrapped(), operation()));
        assert_eq!(routed.held(&wrapped(), operation()), None);
        assert_eq!(own.asked(), Vec::<&str>::new());
    }
}
