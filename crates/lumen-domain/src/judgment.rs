//! Jev judgment types (`docs/ai/model-contract.md`, ADR-0020).
//!
//! A [`Judgment`] is evidence produced by a probabilistic model. Its vocabulary is
//! closed: no paths, commands, actions or numeric scores. Provider-side schema
//! enforcement is a convenience; [`Judgment::validate_for`] is the guarantee.

use std::collections::BTreeSet;
use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::{
    EvidenceHash, EvidenceId, JevEffect, PayloadHash, PolicyVersion, PromptVersion, SchemaVersion,
    SourceName, Timestamp, UntrustedText,
};

/// Reference to the item a request is about (opaque, 1–64 bytes of visible
/// ASCII), echoed back by the model.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ItemRef(String);

/// Error returned for an invalid [`ItemRef`] or [`ModelId`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{kind} must be 1-{max} bytes of visible ASCII")]
pub struct OpaqueIdError {
    kind: &'static str,
    max: usize,
}

fn validate_opaque(kind: &'static str, max: usize, value: &str) -> Result<(), OpaqueIdError> {
    if !value.is_empty() && value.len() <= max && value.bytes().all(|b| b.is_ascii_graphic()) {
        Ok(())
    } else {
        Err(OpaqueIdError { kind, max })
    }
}

macro_rules! opaque_id {
    ($name:ident, $kind:literal, $max:literal) => {
        impl $name {
            /// Validates and wraps the identifier.
            ///
            /// # Errors
            ///
            /// Returns [`OpaqueIdError`] if it is empty, too long, or not visible ASCII.
            pub fn new(value: impl Into<String>) -> Result<Self, OpaqueIdError> {
                let value = value.into();
                validate_opaque($kind, $max, &value)?;
                Ok(Self(value))
            }

            /// The identifier text.
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl Serialize for $name {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.serialize_str(&self.0)
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                Self::new(String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
            }
        }
    };
}

opaque_id!(ItemRef, "item reference", 64);

/// Exact model identifier, e.g. `claude-sonnet-5-5` (1–128 bytes of visible ASCII).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ModelId(String);

opaque_id!(ModelId, "model ID", 128);

/// The model's assessment of an item.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum Assessment {
    /// The evidence suggests the item can be removed.
    LikelyRemovable,
    /// The evidence suggests the item is needed.
    LikelyNeeded,
    /// The evidence is insufficient or contradictory.
    Uncertain,
}

/// Verbalized confidence bucket. Mapped to empirical precision by calibration;
/// never used raw.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum JudgmentConfidence {
    /// Low.
    Low,
    /// Medium.
    Medium,
    /// High.
    High,
}

/// Closed list of reasons a judgment may cite (`ai/schemas/reason-codes.json`).
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ReasonCode {
    /// The owning application appears to be gone.
    OwnerAppAbsent,
    /// The owning application is installed.
    OwnerAppPresent,
    /// The content is a cache that regenerates on demand.
    RegeneratableCache,
    /// The content cannot be regenerated.
    NotRegeneratable,
    /// Nothing references or uses the item.
    NoActiveReference,
    /// Something references or uses the item.
    ActiveReference,
    /// A vendor declared the item disposable.
    VendorDeclaredCache,
    /// The item matches a developer knowledge-base entry.
    KnowledgeBaseMatch,
    /// Not modified or used for a long time.
    StaleByTime,
    /// Used recently.
    RecentlyUsed,
    /// Appears to contain user-created data.
    ContainsUserData,
    /// Appears to contain credentials or key material.
    CredentialOrKeyMaterial,
    /// Appears to be part of the operating system.
    SystemComponent,
    /// Executable without a valid signature.
    UnsignedExecutable,
    /// Cloud-backed; removal may propagate.
    CloudBacked,
    /// The evidence does not support a conclusion.
    InsufficientEvidence,
    /// Metadata looks crafted to influence the judgment.
    SuspiciousMetadata,
}

/// A validated judgment (`jev.judgment/1`). Unknown fields are rejected, as the
/// contract's `additionalProperties: false` requires.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Judgment {
    /// Echo of the request's item reference.
    pub item_ref: ItemRef,
    /// Assessment.
    pub assessment: Assessment,
    /// Verbalized confidence.
    pub confidence: JudgmentConfidence,
    /// Reasons (1–6, closed list).
    pub reason_codes: BTreeSet<ReasonCode>,
    /// Evidence the judgment relied on; must be a subset of what was sent.
    pub evidence_refs: BTreeSet<EvidenceId>,
    /// Whether the model saw text trying to influence it.
    pub injection_suspected: bool,
    /// Display-only rationale (at most [`Judgment::MAX_RATIONALE_CHARS`]).
    pub rationale: UntrustedText,
}

/// Why a judgment was rejected.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    thiserror::Error,
    schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum JudgmentRejection {
    /// `item_ref` does not match the request.
    #[error("judgment is for a different item")]
    ItemMismatch,
    /// No reason codes, or more than six.
    #[error("judgment must cite 1-6 reason codes")]
    ReasonCount,
    /// No evidence references.
    #[error("judgment must cite evidence")]
    NoEvidence,
    /// An evidence reference was not in the request.
    #[error("judgment cites evidence that was not in the request")]
    UnknownEvidence,
    /// The rationale exceeds the length limit or was truncated.
    #[error("judgment rationale is too long")]
    RationaleTooLong,
}

impl Judgment {
    /// Maximum rationale length in characters (Claude does not enforce
    /// `maxLength`, so this is checked here).
    pub const MAX_RATIONALE_CHARS: usize = 280;

    /// Validates the judgment against the request it answers.
    ///
    /// # Errors
    ///
    /// Returns the first [`JudgmentRejection`] that applies.
    pub fn validate_for(
        &self,
        item: &ItemRef,
        sent_evidence: &BTreeSet<EvidenceId>,
    ) -> Result<(), JudgmentRejection> {
        if &self.item_ref != item {
            return Err(JudgmentRejection::ItemMismatch);
        }
        if !(1..=6).contains(&self.reason_codes.len()) {
            return Err(JudgmentRejection::ReasonCount);
        }
        if self.evidence_refs.is_empty() {
            return Err(JudgmentRejection::NoEvidence);
        }
        if !self.evidence_refs.is_subset(sent_evidence) {
            return Err(JudgmentRejection::UnknownEvidence);
        }
        if self.rationale.is_truncated()
            || self.rationale.raw().chars().count() > Self::MAX_RATIONALE_CHARS
        {
            return Err(JudgmentRejection::RationaleTooLong);
        }
        Ok(())
    }
}

/// Which judge produced a judgment.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
pub struct JudgeDescriptor {
    /// Provider adapter, e.g. `anthropic` or `apple.foundation-models`.
    pub provider: SourceName,
    /// Pinned model ID.
    pub model: ModelId,
    /// Model build or OS version, when the provider exposes one.
    pub model_version: Option<UntrustedText>,
    /// Prompt version.
    pub prompt_version: PromptVersion,
}

/// Outcome of validating a model response.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "result", content = "reason", rename_all = "snake_case")]
pub enum TraceValidation {
    /// The judgment was valid and available to the policy.
    Accepted,
    /// The judgment was rejected and ignored.
    Rejected(JudgmentRejection),
    /// No usable response (refusal, truncation, schema failure, timeout,
    /// unexpected model).
    Unavailable,
}

/// Record of one judgment that a policy decision consumed (`jev.trace/1`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
pub struct JevTrace {
    /// When the judgment was requested.
    pub requested_at: Timestamp,
    /// Judge that answered.
    pub judge: JudgeDescriptor,
    /// Policy version that consumed the judgment.
    pub policy_version: PolicyVersion,
    /// Request schema.
    pub input_schema: SchemaVersion,
    /// Response schema.
    pub output_schema: SchemaVersion,
    /// Evidence bundle the request was built from.
    pub evidence_hash: EvidenceHash,
    /// Hash of the exact request bytes.
    pub request_hash: PayloadHash,
    /// Hash of the exact response bytes, if any.
    pub response_hash: Option<PayloadHash>,
    /// Validation outcome.
    pub validation: TraceValidation,
    /// The judgment, when accepted.
    pub judgment: Option<Judgment>,
    /// Calibrated precision lower bound for the judgment's class, in basis points
    /// (0–10 000), when a calibration map exists. Integers keep traces hashable.
    pub calibrated_precision_bp: Option<u16>,
    /// What the judgment did to the decision.
    pub policy_effect: JevEffect,
    /// Round-trip latency in milliseconds.
    pub latency_ms: u32,
}

manual_schema!(ItemRef, "ItemRef", { "type": "string", "pattern": "^[!-~]{1,64}$" });
manual_schema!(ModelId, "ModelId", { "type": "string", "pattern": "^[!-~]{1,128}$" });

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(n: u8) -> EvidenceId {
        format!("b3:{}", format!("{n:02x}").repeat(32))
            .parse()
            .unwrap_or_else(|_| unreachable!())
    }

    fn judgment() -> Judgment {
        Judgment {
            item_ref: ItemRef::new("c_01").unwrap_or_else(|_| unreachable!()),
            assessment: Assessment::LikelyRemovable,
            confidence: JudgmentConfidence::Medium,
            reason_codes: BTreeSet::from([
                ReasonCode::OwnerAppAbsent,
                ReasonCode::RegeneratableCache,
            ]),
            evidence_refs: BTreeSet::from([ev(1), ev(2)]),
            injection_suspected: false,
            rationale: UntrustedText::new("Cache of an application that is no longer installed."),
        }
    }

    fn sent() -> BTreeSet<EvidenceId> {
        BTreeSet::from([ev(1), ev(2), ev(3)])
    }

    #[test]
    fn valid_judgment_passes() {
        assert_eq!(
            judgment().validate_for(&judgment().item_ref, &sent()),
            Ok(())
        );
    }

    #[test]
    fn rejects_contract_violations() {
        let item = judgment().item_ref;
        let other = ItemRef::new("c_02").unwrap_or_else(|_| unreachable!());
        assert_eq!(
            judgment().validate_for(&other, &sent()),
            Err(JudgmentRejection::ItemMismatch)
        );

        let mut j = judgment();
        j.evidence_refs.insert(ev(9));
        assert_eq!(
            j.validate_for(&item, &sent()),
            Err(JudgmentRejection::UnknownEvidence),
            "hallucinated evidence"
        );

        let mut j = judgment();
        j.reason_codes.clear();
        assert_eq!(
            j.validate_for(&item, &sent()),
            Err(JudgmentRejection::ReasonCount)
        );

        let mut j = judgment();
        j.evidence_refs.clear();
        assert_eq!(
            j.validate_for(&item, &sent()),
            Err(JudgmentRejection::NoEvidence)
        );

        let mut j = judgment();
        j.rationale = UntrustedText::new("x".repeat(Judgment::MAX_RATIONALE_CHARS + 1));
        assert_eq!(
            j.validate_for(&item, &sent()),
            Err(JudgmentRejection::RationaleTooLong)
        );
    }

    #[test]
    fn output_vocabulary_is_closed() {
        // Unknown enum values and extra fields cannot sneak through deserialization.
        let ok = serde_json::to_value(judgment()).unwrap_or_else(|_| unreachable!());
        let mut bad = ok.clone();
        bad["assessment"] = serde_json::json!("delete_now");
        assert!(serde_json::from_value::<Judgment>(bad).is_err());
        let mut bad = ok.clone();
        bad["reason_codes"] = serde_json::json!(["run_rm_rf"]);
        assert!(serde_json::from_value::<Judgment>(bad).is_err());
        let mut bad = ok;
        bad["command"] = serde_json::json!("rm -rf ~");
        assert!(
            serde_json::from_value::<Judgment>(bad).is_err(),
            "extra fields are rejected"
        );
    }

    #[test]
    fn identifiers_are_validated() {
        assert!(ItemRef::new("").is_err());
        assert!(ItemRef::new("has space").is_err());
        assert!(ModelId::new("claude-sonnet-5-5").is_ok());
        assert!(serde_json::from_str::<ModelId>("\"bad\\nmodel\"").is_err());
    }
}
