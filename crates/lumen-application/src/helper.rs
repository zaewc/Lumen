//! What Lumen's privileged helper accepts and answers (ADR-0063 rule 5).
//!
//! A request and its answer are one JSON object each. The helper treats every
//! request as untrusted input: anything it cannot read is refused, and who is
//! asking comes from the connection (the caller's user ID), never from the
//! request. The first step (roadmap 16.3b) has one verb, `status`; moving,
//! restoring and finalizing come with the helper's store.
//!
//! [`answer`] is the whole decision, free of IO, so it is tested without
//! privileges in fixtures (ADR-0063 rule 9).

use serde::{Deserialize, Serialize};

/// The protocol version; a client and helper that differ set the helper up
/// again.
pub const PROTOCOL: u32 = 1;

/// A request to the helper.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "verb", rename_all = "snake_case")]
#[non_exhaustive]
pub enum HelperRequest {
    /// Which helper answers, and whom it sees asking.
    Status,
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
}

/// The helper's answer to `body`, sent by the account `caller_uid`, from a
/// helper of `version`.
#[must_use]
pub fn answer(body: &str, caller_uid: u32, version: &str) -> String {
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
    };
    serde_json::to_string(&response).unwrap_or_else(|_| {
        // Serializing these plain enums cannot fail; refuse rather than panic.
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
    use super::*;

    fn refused(reason: HelperRefusal) -> HelperResponse {
        HelperResponse::Refused { reason }
    }

    #[test]
    fn status_reports_the_protocol_version_and_the_caller_from_the_connection() {
        let body = request(&HelperRequest::Status);
        assert_eq!(body, r#"{"verb":"status"}"#);
        assert_eq!(
            response(&answer(&body, 501, "0.10.0")),
            Some(HelperResponse::Status {
                protocol: PROTOCOL,
                version: "0.10.0".into(),
                caller_uid: 501,
            })
        );
    }

    #[test]
    fn a_caller_named_in_the_request_is_ignored() {
        let answered = response(&answer(r#"{"verb":"status","caller_uid":0}"#, 501, "1"));
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
                response(&answer(body, 501, "1")),
                Some(refused(HelperRefusal::Malformed)),
                "{body}"
            );
        }
    }

    #[test]
    fn a_root_caller_is_refused() {
        assert_eq!(
            response(&answer(r#"{"verb":"status"}"#, 0, "1")),
            Some(refused(HelperRefusal::RootCaller))
        );
    }

    #[test]
    fn a_client_does_not_read_an_answer_it_does_not_know() {
        assert_eq!(response(r#"{"kind":"moved"}"#), None);
        assert_eq!(response("not json"), None);
    }
}
