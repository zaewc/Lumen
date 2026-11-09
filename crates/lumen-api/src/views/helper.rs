//! Lumen's privileged helper (ADR-0063): where its registration stands and
//! whether it answers. The page turns it on and off by command; no view carries
//! anything to act on.

use lumen_app::helper::{HelperReach, HelperRegistration, HelperState};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Where the helper's registration stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum HelperRegistrationView {
    /// This system has no helper (only macOS, so far).
    Unsupported,
    /// It is not set up.
    NotRegistered,
    /// It is set up and allowed.
    Enabled,
    /// It is set up, but waits for an administrator to allow it in System
    /// Settings › Login Items, or was turned off there.
    RequiresApproval,
    /// macOS cannot find it in this copy of Lumen.
    NotFound,
}

/// What asking the helper for its status gave.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum HelperReachView {
    /// Not asked: it is not enabled.
    NotAsked,
    /// It answered.
    Answered {
        /// Its version; display-escaped.
        version: String,
        /// It matches this app; otherwise turn it off and on again.
        current: bool,
    },
    /// This build is not signed with a Developer ID, so it cannot use the
    /// helper.
    Unsigned,
    /// It did not answer.
    Unavailable,
    /// It is not signed by the same team as this app.
    NotSignedAsRequired,
    /// It refused the request.
    Refused,
    /// Its answer could not be read.
    Failed,
}

/// Lumen's helper as it stands now.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct HelperView {
    /// Its registration.
    pub registration: HelperRegistrationView,
    /// Its answer, when enabled.
    pub reach: HelperReachView,
}

impl From<&HelperState> for HelperView {
    fn from(state: &HelperState) -> Self {
        Self {
            registration: match state.registration {
                HelperRegistration::Unsupported => HelperRegistrationView::Unsupported,
                HelperRegistration::NotRegistered => HelperRegistrationView::NotRegistered,
                HelperRegistration::Enabled => HelperRegistrationView::Enabled,
                HelperRegistration::RequiresApproval => HelperRegistrationView::RequiresApproval,
                HelperRegistration::NotFound => HelperRegistrationView::NotFound,
            },
            reach: match &state.reach {
                HelperReach::NotAsked => HelperReachView::NotAsked,
                HelperReach::Answered {
                    version, current, ..
                } => HelperReachView::Answered {
                    version: lumen_domain::UntrustedText::new(version.clone()).display(),
                    current: *current,
                },
                HelperReach::Unsigned => HelperReachView::Unsigned,
                HelperReach::Unavailable => HelperReachView::Unavailable,
                HelperReach::NotSignedAsRequired => HelperReachView::NotSignedAsRequired,
                HelperReach::Refused => HelperReachView::Refused,
                HelperReach::Failed => HelperReachView::Failed,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_state_is_tagged_and_the_version_escaped() {
        let view = HelperView::from(&HelperState {
            registration: HelperRegistration::Enabled,
            reach: HelperReach::Answered {
                protocol: 1,
                version: "0.11.0\u{202e}".into(),
                current: false,
            },
        });
        let json = serde_json::to_value(view).unwrap_or_default();
        assert_eq!(json["registration"], "enabled");
        assert_eq!(json["reach"]["kind"], "answered");
        assert_eq!(json["reach"]["version"], "0.11.0⟦U+202E⟧");
        assert_eq!(json["reach"]["current"], false);
        let waiting = HelperView::from(&HelperState {
            registration: HelperRegistration::RequiresApproval,
            reach: HelperReach::NotAsked,
        });
        let json = serde_json::to_value(waiting).unwrap_or_default();
        assert_eq!(json["registration"], "requires_approval");
        assert_eq!(json["reach"], serde_json::json!({"kind": "not_asked"}));
    }
}
