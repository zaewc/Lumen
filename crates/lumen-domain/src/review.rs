//! Descriptive review categories. These never authorize an action or change a verdict.

use serde::{Deserialize, Serialize};

/// Wire contract for the categories exposed alongside a decision's fired rules.
pub const REVIEW_REASONS_SCHEMA: &str = "review-reasons/1";

/// Stable categories of uncertainty, independent of localized explanations.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ReviewReason {
    /// No source identified the owning application or tool.
    UnknownOwner,
    /// A process is using the item; this normally makes the verdict KEEP.
    ActiveProcess,
    /// Hard links or shared blocks make the resource shared.
    SharedResource,
    /// The item may hold data that cannot be regenerated.
    PossibleUserData,
    /// An observation is too old to answer the current question.
    StaleEvidence,
    /// Physical space reclamation cannot be measured exactly.
    UncertainReclaimableSize,
    /// Only the owning tool's supported command may clean the item.
    ToolRequired,
    /// The optional AI judge advised caution.
    AiCaution,
    /// No more specific category is supported by the recorded evidence.
    Unclassified,
}

impl ReviewReason {
    /// Stable wire code. Human explanations come from localized fired rules.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::UnknownOwner => "unknown_owner",
            Self::ActiveProcess => "active_process",
            Self::SharedResource => "shared_resource",
            Self::PossibleUserData => "possible_user_data",
            Self::StaleEvidence => "stale_evidence",
            Self::UncertainReclaimableSize => "uncertain_reclaimable_size",
            Self::ToolRequired => "tool_required",
            Self::AiCaution => "ai_caution",
            Self::Unclassified => "unclassified",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable_codes_round_trip() -> Result<(), serde_json::Error> {
        for code in [
            ReviewReason::UnknownOwner,
            ReviewReason::ActiveProcess,
            ReviewReason::SharedResource,
            ReviewReason::PossibleUserData,
            ReviewReason::StaleEvidence,
            ReviewReason::UncertainReclaimableSize,
            ReviewReason::ToolRequired,
            ReviewReason::AiCaution,
            ReviewReason::Unclassified,
        ] {
            assert_eq!(
                serde_json::from_str::<ReviewReason>(&serde_json::to_string(&code)?)?,
                code
            );
            assert_eq!(serde_json::to_value(code)?, code.as_str());
        }
        assert_eq!(
            serde_json::to_string(&ReviewReason::UnknownOwner)?,
            "\"unknown_owner\""
        );
        Ok(())
    }
}
