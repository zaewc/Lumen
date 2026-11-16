//! The helper's `locate`, `restore` and `finalize` verbs (ADR-0063 rules 5
//! and 6, roadmap 16.3d).
//!
//! Each names only an operation. The helper acts on its own record of what it
//! holds for the caller ([`HeldRecord`], written next to the item in the
//! caller's root-owned store when it was quarantined), never on a path or an
//! executor record from the request:
//!
//! 1. The request must name the helper's own [`PROTOCOL`]. The caller is
//!    any account but root, from the connection (ADR-0063 as amended
//!    2026-11-16).
//! 2. The record must be in the caller's store, readable, of this helper's
//!    form, for this caller and this operation. Otherwise the helper holds
//!    nothing for them under that operation, and does nothing.
//! 3. The executor acts on the record's own executor record:
//!    - `locate` says where the item is: back at its source, in the store, or
//!      neither (unknown, never assumed gone);
//!    - `restore` moves it back to where it was, by handle and without
//!      replacing anything. The record stays, so a lost journal entry can be
//!      reconciled by `locate`;
//!    - `finalize` removes it for good, only if it is in the store and every
//!      process is asked and none uses it (with full rights only `No`
//!      passes), after the executor checks the whole held tree.

use lumen_domain::OperationId;
use serde::{Deserialize, Serialize};

use super::quarantine::{HELD_SCHEMA, HeldRecord, HelperHost};
use super::{HelperRefusal, HelperResponse, PROTOCOL};
use crate::ports::{InUse, Located, NotRestored, PreparedItem, Removal};

/// A request about an item the helper holds: only its operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HeldRequest {
    /// The client's [`PROTOCOL`].
    pub protocol: u32,
    /// The operation the item was quarantined as.
    pub operation: OperationId,
}

/// Where a held item is, as [`Located`] says it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Whereabouts {
    /// Back at its source, unchanged.
    AtSource,
    /// In the store, as recorded.
    InStore,
    /// Neither, or changed: unknown, never assumed gone.
    Unknown,
}

impl From<Located> for Whereabouts {
    fn from(located: Located) -> Self {
        match located {
            Located::AtSource => Self::AtSource,
            Located::InStore => Self::InStore,
            Located::Unknown => Self::Unknown,
        }
    }
}

impl From<Whereabouts> for Located {
    fn from(whereabouts: Whereabouts) -> Self {
        match whereabouts {
            Whereabouts::AtSource => Self::AtSource,
            Whereabouts::InStore => Self::InStore,
            Whereabouts::Unknown => Self::Unknown,
        }
    }
}

/// Why a restore did not happen, as [`NotRestored`] says it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RestoreRefusal {
    /// Something now has the item's name; the person decides.
    NameTaken,
    /// The original location cannot be reached safely.
    OriginUnreachable,
    /// The held item is missing or changed.
    ItemChanged,
    /// The move back or a check failed.
    Failed,
}

impl From<NotRestored> for RestoreRefusal {
    fn from(reason: NotRestored) -> Self {
        match reason {
            NotRestored::NameTaken => Self::NameTaken,
            NotRestored::OriginUnreachable => Self::OriginUnreachable,
            NotRestored::ItemChanged => Self::ItemChanged,
            NotRestored::Failed => Self::Failed,
        }
    }
}

impl From<RestoreRefusal> for NotRestored {
    fn from(reason: RestoreRefusal) -> Self {
        match reason {
            RestoreRefusal::NameTaken => Self::NameTaken,
            RestoreRefusal::OriginUnreachable => Self::OriginUnreachable,
            RestoreRefusal::ItemChanged => Self::ItemChanged,
            RestoreRefusal::Failed => Self::Failed,
        }
    }
}

/// Why a held item was not removed for good.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FinalizeRefusal {
    /// It is back at its source; nothing was removed.
    AtSource,
    /// It is neither in the store nor at its source; nothing was removed.
    NotFound,
    /// A process uses it, or that could not be known for every process;
    /// nothing was removed.
    InUse,
    /// The removal failed before removing anything; the item is intact.
    FailedIntact,
    /// The removal stopped part way; finalizing again finishes it.
    FailedPartly,
}

/// The helper's record of `operation` for `caller`, if it holds one: in the
/// caller's store, of this form, naming this caller and operation.
fn record(host: &dyn HelperHost, caller: u32, operation: OperationId) -> Option<HeldRecord> {
    let bytes = host.record(caller, operation)?;
    let record: HeldRecord = serde_json::from_slice(&bytes).ok()?;
    (record.schema == HELD_SCHEMA && record.caller == caller && record.operation == operation)
        .then_some(record)
}

/// The record and its executor record, or why the request is answered
/// without acting.
fn admitted(
    caller: u32,
    request: &HeldRequest,
    host: &dyn HelperHost,
) -> Result<PreparedItem, HelperResponse> {
    if request.protocol != PROTOCOL {
        return Err(HelperResponse::Refused {
            reason: HelperRefusal::OtherProtocol,
        });
    }
    record(host, caller, request.operation)
        .and_then(|record| serde_json::to_vec(&record.prepared).ok())
        .map(PreparedItem)
        .ok_or(HelperResponse::NotHeld)
}

/// Answers `locate` from `caller`.
pub(super) fn locate(caller: u32, request: &HeldRequest, host: &dyn HelperHost) -> HelperResponse {
    match admitted(caller, request, host) {
        Ok(item) => HelperResponse::Located {
            whereabouts: host.locate(caller, &item, request.operation).into(),
        },
        Err(response) => response,
    }
}

/// Answers `restore` from `caller`.
pub(super) fn restore(caller: u32, request: &HeldRequest, host: &dyn HelperHost) -> HelperResponse {
    let item = match admitted(caller, request, host) {
        Ok(item) => item,
        Err(response) => return response,
    };
    match host.restore(caller, &item, request.operation) {
        Ok(restored) => HelperResponse::Restored {
            parent_replaced: restored.parent_replaced,
            matches_record: restored.matches_record,
        },
        Err(failure) => HelperResponse::NotRestored {
            reason: failure.reason.into(),
            left: failure.left,
        },
    }
}

/// Answers `finalize` from `caller`.
pub(super) fn finalize(
    caller: u32,
    request: &HeldRequest,
    host: &dyn HelperHost,
) -> HelperResponse {
    let item = match admitted(caller, request, host) {
        Ok(item) => item,
        Err(response) => return response,
    };
    let refused = |reason| HelperResponse::NotFinalized { reason };
    match host.locate(caller, &item, request.operation) {
        Located::InStore => {}
        Located::AtSource => return refused(FinalizeRefusal::AtSource),
        Located::Unknown => return refused(FinalizeRefusal::NotFound),
    }
    // With full rights every process is seen, so only `No` passes.
    let free = host
        .held(caller, &item, request.operation)
        .is_some_and(|(path, identity)| host.in_use(&path, &identity) == InUse::No);
    if !free {
        return refused(FinalizeRefusal::InUse);
    }
    match host.finalize(caller, &item, request.operation) {
        // Only the record may remain, and the item is gone.
        Ok(()) | Err(Removal::Item) => HelperResponse::Finalized,
        Err(Removal::Nothing) => refused(FinalizeRefusal::FailedIntact),
        Err(Removal::Partly) => refused(FinalizeRefusal::FailedPartly),
    }
}

#[cfg(test)]
mod tests {
    use lumen_domain::{EvidenceHash, ItemLocation, Timestamp};

    use super::*;
    use crate::helper::quarantine::fake::{APP, CALLER, Call, Host, identity, raw};
    use crate::helper::{HelperRequest, answer, request as encode, response};
    use crate::ports::{RestoreFailure, RestoredItem};

    const OPERATION: &str = "0190f1c2-7a3b-7c4d-8e5f-6a7b8c9d0e1f";

    fn operation() -> OperationId {
        OPERATION.parse().unwrap_or_else(|_| unreachable!())
    }

    fn held_record(caller: u32, operation: OperationId) -> Vec<u8> {
        serde_json::to_vec(&HeldRecord {
            schema: HELD_SCHEMA,
            operation,
            caller,
            path: raw(APP),
            identity: identity(7),
            evidence_hash: EvidenceHash::from_bytes([3; 32]),
            prepared: serde_json::json!({"leaf": [87]}),
            moved_at: Timestamp::UNIX_EPOCH,
            helper_version: "0.11.0".into(),
        })
        .unwrap_or_default()
    }

    /// A host holding the record of [`OPERATION`] for [`CALLER`].
    fn holding() -> Host {
        Host {
            record: Some(held_record(CALLER, operation())),
            ..Host::eligible()
        }
    }

    fn ask(host: &Host, verb: fn(HeldRequest) -> HelperRequest) -> Option<HelperResponse> {
        let body = encode(&verb(HeldRequest {
            protocol: PROTOCOL,
            operation: operation(),
        }));
        response(&answer(&body, CALLER, "0.11.0", host))
    }

    fn acted(host: &Host) -> bool {
        host.calls().iter().any(|call| {
            matches!(
                call,
                Call::Locate(..) | Call::Restore(..) | Call::Finalize(..)
            )
        })
    }

    const VERBS: [fn(HeldRequest) -> HelperRequest; 3] = [
        HelperRequest::Locate,
        HelperRequest::Restore,
        HelperRequest::Finalize,
    ];

    #[test]
    fn each_verb_acts_on_the_helpers_own_record() {
        let host = holding();
        assert_eq!(
            ask(&host, HelperRequest::Locate),
            Some(HelperResponse::Located {
                whereabouts: Whereabouts::InStore
            })
        );
        assert_eq!(
            ask(&host, HelperRequest::Restore),
            Some(HelperResponse::Restored {
                parent_replaced: false,
                matches_record: true
            })
        );
        assert_eq!(
            ask(&host, HelperRequest::Finalize),
            Some(HelperResponse::Finalized)
        );
        let item = PreparedItem(br#"{"leaf":[87]}"#.to_vec());
        let calls = host.calls();
        assert!(calls.contains(&Call::Record(CALLER, operation())));
        assert!(calls.contains(&Call::Restore(CALLER, item.0.clone(), operation())));
        assert!(calls.contains(&Call::Finalize(CALLER, item.0, operation())));
        assert!(calls.contains(&Call::InUse));
    }

    #[test]
    fn without_a_matching_record_nothing_is_done() {
        let other: OperationId = "0190f1c2-7a3b-7c4d-8e5f-6a7b8c9d0e2f"
            .parse()
            .unwrap_or_else(|_| unreachable!());
        let mut wrong_schema: serde_json::Value =
            serde_json::from_slice(&held_record(CALLER, operation())).unwrap_or_default();
        wrong_schema["schema"] = serde_json::json!(HELD_SCHEMA + 1);
        for record in [
            None,
            Some(b"not json".to_vec()),
            Some(held_record(CALLER + 1, operation())),
            Some(held_record(CALLER, other)),
            Some(serde_json::to_vec(&wrong_schema).unwrap_or_default()),
        ] {
            for verb in VERBS {
                let host = Host {
                    record: record.clone(),
                    ..Host::eligible()
                };
                assert_eq!(ask(&host, verb), Some(HelperResponse::NotHeld));
                assert!(!acted(&host), "{record:?}");
            }
        }
    }

    #[test]
    fn another_protocol_or_a_root_caller_is_refused_before_reading() {
        for verb in VERBS {
            let host = holding();
            let body = encode(&verb(HeldRequest {
                protocol: PROTOCOL - 1,
                operation: operation(),
            }));
            assert_eq!(
                response(&answer(&body, CALLER, "1", &host)),
                Some(HelperResponse::Refused {
                    reason: HelperRefusal::OtherProtocol
                })
            );
            assert_eq!(host.calls(), []);
            assert_eq!(
                response(&answer(&body, 0, "1", &host)),
                Some(HelperResponse::Refused {
                    reason: HelperRefusal::RootCaller
                })
            );
        }
    }

    #[test]
    fn a_path_or_record_in_the_request_makes_it_malformed() {
        let host = holding();
        for extra in [r#""path":"/etc","#, r#""prepared":{},"#, r#""caller":0,"#] {
            let body = encode(&HelperRequest::Restore(HeldRequest {
                protocol: PROTOCOL,
                operation: operation(),
            }))
            .replacen(r#""protocol""#, &format!(r#"{extra}"protocol""#), 1);
            assert_eq!(
                response(&answer(&body, CALLER, "1", &host)),
                Some(HelperResponse::Refused {
                    reason: HelperRefusal::Malformed
                }),
                "{body}"
            );
        }
        assert_eq!(host.calls(), []);
    }

    #[test]
    fn a_failed_restore_says_where_the_item_is() {
        for left in [ItemLocation::Quarantine, ItemLocation::Source] {
            let host = Host {
                restored: Err(RestoreFailure {
                    reason: NotRestored::NameTaken,
                    left,
                }),
                ..holding()
            };
            assert_eq!(
                ask(&host, HelperRequest::Restore),
                Some(HelperResponse::NotRestored {
                    reason: RestoreRefusal::NameTaken,
                    left
                })
            );
        }
        let restored = Host {
            restored: Ok(RestoredItem {
                parent_replaced: false,
                matches_record: false,
            }),
            ..holding()
        };
        assert_eq!(
            ask(&restored, HelperRequest::Restore),
            Some(HelperResponse::Restored {
                parent_replaced: false,
                matches_record: false
            })
        );
    }

    #[test]
    fn finalize_removes_only_a_held_item_nothing_uses() {
        let finalized = |host: &Host| host.calls().iter().any(|c| matches!(c, Call::Finalize(..)));
        for (located, reason) in [
            (Located::AtSource, FinalizeRefusal::AtSource),
            (Located::Unknown, FinalizeRefusal::NotFound),
        ] {
            let host = Host {
                located,
                ..holding()
            };
            assert_eq!(
                ask(&host, HelperRequest::Finalize),
                Some(HelperResponse::NotFinalized { reason })
            );
            assert!(!finalized(&host));
        }
        for in_use in [
            InUse::Partial { unseen: Some(1) },
            InUse::Unknown { reason: "test" },
            InUse::Yes(Vec::new()),
        ] {
            let host = Host {
                in_use: in_use.clone(),
                ..holding()
            };
            assert_eq!(
                ask(&host, HelperRequest::Finalize),
                Some(HelperResponse::NotFinalized {
                    reason: FinalizeRefusal::InUse
                }),
                "{in_use:?}"
            );
            assert!(!finalized(&host), "{in_use:?}");
        }
        let unheld = Host {
            held: None,
            ..holding()
        };
        assert_eq!(
            ask(&unheld, HelperRequest::Finalize),
            Some(HelperResponse::NotFinalized {
                reason: FinalizeRefusal::InUse
            })
        );
        assert!(!finalized(&unheld));
        for (removed, answer) in [
            (Removal::Item, HelperResponse::Finalized),
            (
                Removal::Nothing,
                HelperResponse::NotFinalized {
                    reason: FinalizeRefusal::FailedIntact,
                },
            ),
            (
                Removal::Partly,
                HelperResponse::NotFinalized {
                    reason: FinalizeRefusal::FailedPartly,
                },
            ),
        ] {
            let host = Host {
                finalized: Err(removed),
                ..holding()
            };
            assert_eq!(ask(&host, HelperRequest::Finalize), Some(answer));
        }
    }

    #[test]
    fn the_wire_forms_are_stable() {
        let body = encode(&HelperRequest::Finalize(HeldRequest {
            protocol: 3,
            operation: operation(),
        }));
        assert_eq!(
            body,
            format!(r#"{{"verb":"finalize","protocol":3,"operation":"{OPERATION}"}}"#)
        );
        let json = serde_json::to_string(&HelperResponse::NotRestored {
            reason: RestoreRefusal::NameTaken,
            left: ItemLocation::Quarantine,
        })
        .unwrap_or_default();
        assert_eq!(
            json,
            r#"{"kind":"not_restored","reason":"name_taken","left":"quarantine"}"#
        );
        for located in [Located::AtSource, Located::InStore, Located::Unknown] {
            assert_eq!(Located::from(Whereabouts::from(located)), located);
        }
        for reason in [
            NotRestored::NameTaken,
            NotRestored::OriginUnreachable,
            NotRestored::ItemChanged,
            NotRestored::Failed,
        ] {
            assert_eq!(NotRestored::from(RestoreRefusal::from(reason)), reason);
        }
    }
}
