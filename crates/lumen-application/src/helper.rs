//! What Lumen's privileged helper accepts and answers (ADR-0063 rule 5).
//!
//! A request and its answer are one JSON object each. The helper treats every
//! request as untrusted input: anything it cannot read is refused, and who is
//! asking comes from the connection (the caller's user ID), never from the
//! request. Its verbs so far:
//!
//! - `status` (roadmap 16.3b): which helper answers, and whom it sees asking;
//! - `quarantine` (16.3c): move an app another account owns into the
//!   caller's store, after deciding everything again itself
//!   ([`quarantine`](mod@quarantine), [`gate`]);
//! - `locate`, `restore` and `finalize` (16.3d): act on an item the helper
//!   holds for the caller, named by its operation alone, from the helper's own
//!   record of it ([`held`]).
//!
//! [`answer`] is the whole decision. Everything it reads or does on the
//! computer goes through [`HelperHost`], so it is tested without privileges
//! (ADR-0063 rule 9).
//!
//! Versions: [`PROTOCOL`] 2 added `quarantine`, 3 added `locate`, `restore`
//! and `finalize`, 4 added `forget`. A client asks `status` first and sends
//! the other verbs only to a helper of its own protocol; an older helper does
//! not know a newer verb and answers it as malformed. Each request but
//! `status` also names the client's protocol, and a helper of another
//! refuses it ([`HelperRefusal::OtherProtocol`]).

pub mod gate;
pub mod held;
pub mod quarantine;

use lumen_domain::{
    EvidenceHash, FileIdentity, ItemLocation, OperationId, QuarantineFailure, RawPath,
};
use serde::{Deserialize, Serialize};

pub use held::{FinalizeRefusal, HeldRequest, RestoreRefusal, Whereabouts};
pub use quarantine::{HELD_SCHEMA, HeldRecord, HelperHost};

/// The protocol version; a client and helper that differ set the helper up
/// again.
pub const PROTOCOL: u32 = 4;

/// A request to the helper.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "verb", rename_all = "snake_case")]
#[non_exhaustive]
pub enum HelperRequest {
    /// Which helper answers, and whom it sees asking.
    Status,
    /// Move a planned app another account owns into the caller's store.
    Quarantine(QuarantineRequest),
    /// Where an item the helper holds for the caller is.
    Locate(HeldRequest),
    /// Move an item the helper holds for the caller back to where it was.
    Restore(HeldRequest),
    /// Remove an item the helper holds for the caller for good.
    Finalize(HeldRequest),
    /// Drop the helper's record of an item it put back where it was.
    Forget(HeldRequest),
}

/// A planned item the person confirmed by hash (ADR-0034), as Lumen sends
/// it. Nothing in it is trusted: the helper finds the item again by path and
/// checks its identity, and decides everything else from its own evidence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QuarantineRequest {
    /// The client's [`PROTOCOL`].
    pub protocol: u32,
    /// The operation Lumen journaled before asking: the item's name in the
    /// store, and what restoring or finalizing it will name.
    pub operation: OperationId,
    /// The planned item's path.
    pub path: RawPath,
    /// Its identity when the person confirmed it.
    pub identity: FileIdentity,
    /// The confirmed decision's evidence hash, kept in the helper's record.
    pub evidence_hash: EvidenceHash,
}

/// Why the helper refused a request; nothing was done.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum HelperRefusal {
    /// The request is not one the helper reads: malformed, or an unknown verb.
    Malformed,
    /// The caller runs as root. The helper acts for a person's own account,
    /// and Lumen itself never runs privileged.
    RootCaller,
    /// The request names another protocol than the helper's; set the helper
    /// up again.
    OtherProtocol,
}

/// The helper's answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[non_exhaustive]
pub enum HelperResponse {
    /// The answer to [`HelperRequest::Status`].
    Status {
        /// The helper's [`PROTOCOL`].
        protocol: u32,
        /// The helper's version (Lumen's version it was built with).
        version: String,
        /// The caller's user ID, as the connection reports it.
        caller_uid: u32,
    },
    /// The request was refused.
    Refused {
        /// Why.
        reason: HelperRefusal,
    },
    /// The item is in the caller's store as `operation`, verified, with the
    /// helper's record next to it.
    Quarantined {
        /// The request's operation.
        operation: OperationId,
    },
    /// The helper's decision refused the item; it was not moved.
    Ineligible {
        /// Every reason, at least one.
        reasons: Vec<gate::Reason>,
    },
    /// The move failed; `left` says where the item is.
    Failed {
        /// Why.
        reason: QuarantineFailure,
        /// Where it is.
        left: ItemLocation,
    },
    /// The helper holds nothing for the caller under the request's operation;
    /// nothing was done.
    NotHeld,
    /// The answer to [`HelperRequest::Locate`].
    Located {
        /// Where the item is.
        whereabouts: Whereabouts,
    },
    /// The item is back where it was.
    Restored {
        /// Its original parent had been recreated.
        parent_replaced: bool,
        /// It matched its record in full after the move.
        matches_record: bool,
    },
    /// The item was not restored; `left` says where it is.
    NotRestored {
        /// Why.
        reason: RestoreRefusal,
        /// Where it is.
        left: ItemLocation,
    },
    /// The item is removed for good.
    Finalized,
    /// The item was not removed for good.
    NotFinalized {
        /// Why, and how much was removed.
        reason: FinalizeRefusal,
    },
    /// The record of an item back at its source is gone.
    Forgotten,
    /// The record stays: the item is not back at its source, or the record
    /// could not be removed.
    NotForgotten,
}

/// The helper's answer to `body`, sent by the account `caller_uid`, from a
/// helper of `version` acting through `host`.
#[must_use]
pub fn answer(body: &str, caller_uid: u32, version: &str, host: &dyn HelperHost) -> String {
    let response = match serde_json::from_str::<HelperRequest>(body) {
        Err(_) => HelperResponse::Refused {
            reason: HelperRefusal::Malformed,
        },
        Ok(_) if caller_uid == 0 => HelperResponse::Refused {
            reason: HelperRefusal::RootCaller,
        },
        Ok(HelperRequest::Status) => HelperResponse::Status {
            protocol: PROTOCOL,
            version: version.to_owned(),
            caller_uid,
        },
        Ok(HelperRequest::Quarantine(request)) => {
            quarantine::quarantine(caller_uid, &request, version, host)
        }
        Ok(HelperRequest::Locate(request)) => held::locate(caller_uid, &request, host),
        Ok(HelperRequest::Restore(request)) => held::restore(caller_uid, &request, host),
        Ok(HelperRequest::Finalize(request)) => held::finalize(caller_uid, &request, host),
        Ok(HelperRequest::Forget(request)) => held::forget(caller_uid, &request, host),
    };
    serde_json::to_string(&response).unwrap_or_else(|_| {
        // Serializing these plain types cannot fail; refuse rather than panic.
        r#"{"kind":"refused","reason":"malformed"}"#.to_owned()
    })
}

/// The request's JSON, for the client.
#[must_use]
pub fn request(request: &HelperRequest) -> String {
    serde_json::to_string(request).unwrap_or_else(|_| r#"{"verb":"status"}"#.to_owned())
}

/// The answer read by the client, or `None` if it is not one.
#[must_use]
pub fn response(body: &str) -> Option<HelperResponse> {
    serde_json::from_str(body).ok()
}

#[cfg(test)]
mod tests {
    use super::quarantine::fake::Host;
    use super::*;

    fn refused(reason: HelperRefusal) -> HelperResponse {
        HelperResponse::Refused { reason }
    }

    #[test]
    fn status_reports_the_protocol_version_and_the_caller_from_the_connection() {
        let body = request(&HelperRequest::Status);
        assert_eq!(body, r#"{"verb":"status"}"#);
        assert_eq!(
            response(&answer(&body, 501, "0.10.0", &Host::eligible())),
            Some(HelperResponse::Status {
                protocol: PROTOCOL,
                version: "0.10.0".into(),
                caller_uid: 501,
            })
        );
    }

    #[test]
    fn a_caller_named_in_the_request_is_ignored() {
        let answered = response(&answer(
            r#"{"verb":"status","caller_uid":0}"#,
            501,
            "1",
            &Host::eligible(),
        ));
        assert!(matches!(
            answered,
            Some(HelperResponse::Status {
                caller_uid: 501,
                ..
            })
        ));
    }

    #[test]
    fn anything_unreadable_is_refused() {
        for body in [
            "",
            "status",
            "{}",
            r#"{"verb":"quarantine","path":"/"}"#,
            r#"{"verb":"STATUS"}"#,
            "[]",
        ] {
            assert_eq!(
                response(&answer(body, 501, "1", &Host::eligible())),
                Some(refused(HelperRefusal::Malformed)),
                "{body}"
            );
        }
    }

    #[test]
    fn a_root_caller_is_refused() {
        assert_eq!(
            response(&answer(r#"{"verb":"status"}"#, 0, "1", &Host::eligible())),
            Some(refused(HelperRefusal::RootCaller))
        );
    }

    #[test]
    fn a_client_does_not_read_an_answer_it_does_not_know() {
        assert_eq!(response(r#"{"kind":"moved"}"#), None);
        assert_eq!(response("not json"), None);
    }
}
