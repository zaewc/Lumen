//! Cleanup candidates and the actions that may be proposed for them.
//!
//! A [`CleanupCandidate`] is what a user reviews: what the item is, how much space
//! it really frees, the evidence and relationships behind the recommendation, the
//! policy decision, and the action Lumen would take. Construction enforces that
//! the proposed action agrees with the verdict (ADR-0014, ADR-0015).

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::{
    CleanupMechanism, EvidenceId, FileIdentity, PlatformCapabilities, PolicyDecision, PolicyStage,
    RawPath, ReclaimEstimate, Relationship, Reversibility, SourceName, Subject, Verdict,
};

/// What kind of artifact a candidate is.
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
pub enum Category {
    /// An application's cache.
    AppCache,
    /// Build output of a developer tool (e.g. Xcode `DerivedData`, `target/`).
    DeveloperBuildOutput,
    /// Package or tool cache (npm, pnpm, Cargo, Gradle, Homebrew, …).
    DeveloperCache,
    /// Log files.
    Log,
    /// Temporary files.
    Temporary,
    /// Downloaded installers and archives.
    Download,
    /// Exact duplicate content.
    Duplicate,
    /// Large or old media.
    Media,
    /// Cloud-backed item that can be evicted locally.
    CloudItem,
    /// Service, startup item or package remnant whose owner appears to be gone.
    OrphanedComponent,
    /// Anything not covered above.
    Other,
}

/// How sure the deterministic rules are about the classification (not Jev's
/// confidence, which is recorded separately in its trace).
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
pub enum Confidence {
    /// Weak or partial evidence.
    Low,
    /// Corroborated by more than one source.
    Medium,
    /// Vendor-declared or knowledge-base backed, with complete coverage.
    High,
}

/// An action Lumen can propose.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case")]
#[non_exhaustive]
pub enum CleanupAction {
    /// Move into Lumen's quarantine store.
    QuarantineMove,
    /// Android media trash.
    MediaTrash,
    /// iOS Photos delete into Recently Deleted.
    PhotoLibraryDelete,
    /// Evict or dehydrate a cloud-backed item.
    CloudEvict,
    /// Run a developer tool's own cleanup command.
    ToolCommand {
        /// Knowledge-base entry that defines the command.
        entry: SourceName,
    },
    /// Android system-wide external cache clear.
    SystemCacheClear,
    /// Archive an unused Android app.
    ArchiveApp,
    /// Open the system's own settings screen.
    OpenSystemSettings,
}

impl CleanupAction {
    /// The platform mechanism the action uses.
    pub const fn mechanism(&self) -> CleanupMechanism {
        match self {
            Self::QuarantineMove => CleanupMechanism::QuarantineRename,
            Self::MediaTrash => CleanupMechanism::MediaTrash,
            Self::PhotoLibraryDelete => CleanupMechanism::PhotoLibraryDelete,
            Self::CloudEvict => CleanupMechanism::CloudEvict,
            Self::ToolCommand { .. } => CleanupMechanism::ToolCommand,
            Self::SystemCacheClear => CleanupMechanism::SystemCacheClear,
            Self::ArchiveApp => CleanupMechanism::ArchiveApp,
            Self::OpenSystemSettings => CleanupMechanism::OpenSystemSettings,
        }
    }

    /// Whether the action can be undone.
    pub const fn reversibility(&self) -> Reversibility {
        self.mechanism().reversibility()
    }
}

/// What a candidate refers to: one subject, and the filesystem objects involved.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CandidateTarget {
    /// The artifact's subject (a file or directory, or an application/service for
    /// orphan findings).
    pub subject: Subject,
    /// Filesystem objects the action would affect.
    pub identities: BTreeSet<FileIdentity>,
    /// Paths shown to the user (display only; actions use identities).
    pub paths: Vec<RawPath>,
}

/// Error returned when a candidate's action contradicts its decision.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum CandidateError {
    /// `Keep` candidates offer no action.
    #[error("a keep verdict cannot carry an action")]
    ActionOnKeep,
    /// `Quarantine` candidates must carry an action.
    #[error("a quarantine verdict needs a proposed action")]
    MissingAction,
    /// `Quarantine` may only propose reversible actions; irreversible ones need
    /// individual review.
    #[error("a quarantine verdict cannot propose an irreversible or no-op action ({action:?})")]
    IrreversibleQuarantine {
        /// The offending action.
        action: CleanupAction,
    },
}

/// A reviewed unit of potential cleanup.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema)]
pub struct CleanupCandidate {
    target: CandidateTarget,
    size: ReclaimEstimate,
    category: Category,
    evidence: BTreeSet<EvidenceId>,
    relationships: Vec<Relationship>,
    regeneratable: Option<bool>,
    in_use: Option<bool>,
    confidence: Confidence,
    decision: PolicyDecision,
    action: Option<CleanupAction>,
}

/// Inputs for [`CleanupCandidate::new`].
#[derive(Debug, Clone)]
pub struct CandidateParts {
    /// What the candidate refers to.
    pub target: CandidateTarget,
    /// Reclaim estimate for the target.
    pub size: ReclaimEstimate,
    /// Artifact category.
    pub category: Category,
    /// Evidence behind the recommendation.
    pub evidence: BTreeSet<EvidenceId>,
    /// Relationships used by the explanation.
    pub relationships: Vec<Relationship>,
    /// Whether the content regenerates on demand (`None` = unknown).
    pub regeneratable: Option<bool>,
    /// Whether anything uses it now (`None` = unknown).
    pub in_use: Option<bool>,
    /// Rule-based confidence.
    pub confidence: Confidence,
    /// The policy decision.
    pub decision: PolicyDecision,
    /// Proposed action, if any.
    pub action: Option<CleanupAction>,
}

impl CleanupCandidate {
    /// Builds a candidate, checking that the action agrees with the verdict.
    ///
    /// # Errors
    ///
    /// Returns a [`CandidateError`] if a `Keep` carries an action, a `Quarantine`
    /// has none, or a `Quarantine` proposes an irreversible or no-op action.
    pub fn new(parts: CandidateParts) -> Result<Self, CandidateError> {
        match (parts.decision.verdict(), &parts.action) {
            (Verdict::Keep, Some(_)) => return Err(CandidateError::ActionOnKeep),
            (Verdict::Quarantine, None) => return Err(CandidateError::MissingAction),
            (Verdict::Quarantine, Some(action))
                if !matches!(
                    action.reversibility(),
                    Reversibility::Reversible | Reversibility::Conditional
                ) =>
            {
                return Err(CandidateError::IrreversibleQuarantine {
                    action: action.clone(),
                });
            }
            _ => {}
        }
        Ok(Self {
            target: parts.target,
            size: parts.size,
            category: parts.category,
            evidence: parts.evidence,
            relationships: parts.relationships,
            regeneratable: parts.regeneratable,
            in_use: parts.in_use,
            confidence: parts.confidence,
            decision: parts.decision,
            action: parts.action,
        })
    }

    /// What the candidate refers to.
    pub fn target(&self) -> &CandidateTarget {
        &self.target
    }

    /// Reclaim estimate.
    pub fn size(&self) -> &ReclaimEstimate {
        &self.size
    }

    /// Category.
    pub const fn category(&self) -> Category {
        self.category
    }

    /// Evidence behind the recommendation.
    pub fn evidence(&self) -> &BTreeSet<EvidenceId> {
        &self.evidence
    }

    /// Relationships used by the explanation.
    pub fn relationships(&self) -> &[Relationship] {
        &self.relationships
    }

    /// Whether the content regenerates on demand (`None` = unknown).
    pub const fn regeneratable(&self) -> Option<bool> {
        self.regeneratable
    }

    /// Whether anything uses it now (`None` = unknown).
    pub const fn in_use(&self) -> Option<bool> {
        self.in_use
    }

    /// Rule-based confidence.
    pub const fn confidence(&self) -> Confidence {
        self.confidence
    }

    /// The policy decision.
    pub fn decision(&self) -> &PolicyDecision {
        &self.decision
    }

    /// Proposed action, if any.
    pub fn action(&self) -> Option<&CleanupAction> {
        self.action.as_ref()
    }

    /// Whether a hard protection applies. Derived from the decision so the two can
    /// never disagree.
    pub fn is_protected(&self) -> bool {
        self.decision
            .fired_rules()
            .iter()
            .any(|r| r.stage == PolicyStage::HardProtection)
    }

    /// How reversible the proposed action is (`NotApplicable` without an action).
    pub fn reversibility(&self) -> Reversibility {
        self.action
            .as_ref()
            .map_or(Reversibility::NotApplicable, CleanupAction::reversibility)
    }

    /// Whether the proposed action can run on a host with these capabilities.
    pub fn is_actionable_on(&self, capabilities: &PlatformCapabilities) -> bool {
        self.action
            .as_ref()
            .is_some_and(|a| capabilities.supports(a.mechanism()))
    }
}

impl<'de> Deserialize<'de> for CleanupCandidate {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Wire {
            target: CandidateTarget,
            size: ReclaimEstimate,
            category: Category,
            evidence: BTreeSet<EvidenceId>,
            relationships: Vec<Relationship>,
            regeneratable: Option<bool>,
            in_use: Option<bool>,
            confidence: Confidence,
            decision: PolicyDecision,
            action: Option<CleanupAction>,
        }
        let w = Wire::deserialize(deserializer)?;
        Self::new(CandidateParts {
            target: w.target,
            size: w.size,
            category: w.category,
            evidence: w.evidence,
            relationships: w.relationships,
            regeneratable: w.regeneratable,
            in_use: w.in_use,
            confidence: w.confidence,
            decision: w.decision,
            action: w.action,
        })
        .map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        EvidenceHash, FileId, FiredRule, JevEffect, Platform, PolicyVersion, Risk, RiskLevel,
        RuleId, VolumeId,
    };

    fn decision(verdict: Verdict, stage: PolicyStage) -> PolicyDecision {
        PolicyDecision::new(
            verdict,
            PolicyVersion::new(1, 0, 0),
            EvidenceHash::from_bytes([1; 32]),
            [FiredRule {
                rule: RuleId::new("r.test").unwrap_or_else(|_| unreachable!()),
                stage,
                evidence: BTreeSet::new(),
            }],
            Risk {
                level: RiskLevel::Low,
                factors: BTreeSet::new(),
            },
            JevEffect::None,
        )
        .unwrap_or_else(|e| unreachable!("{e}"))
    }

    fn parts(
        verdict: Verdict,
        stage: PolicyStage,
        action: Option<CleanupAction>,
    ) -> CandidateParts {
        let identity = FileIdentity {
            volume: VolumeId::new("v").unwrap_or_else(|_| unreachable!()),
            file: FileId::new(9),
        };
        CandidateParts {
            target: CandidateTarget {
                subject: Subject::File {
                    identity: identity.clone(),
                },
                identities: BTreeSet::from([identity]),
                paths: vec![],
            },
            size: ReclaimEstimate::default(),
            category: Category::AppCache,
            evidence: BTreeSet::new(),
            relationships: vec![],
            regeneratable: Some(true),
            in_use: Some(false),
            confidence: Confidence::High,
            decision: decision(verdict, stage),
            action,
        }
    }

    fn tool() -> CleanupAction {
        CleanupAction::ToolCommand {
            entry: SourceName::new("devkb.pnpm-store").unwrap_or_else(|_| unreachable!()),
        }
    }

    #[test]
    fn keep_offers_nothing() {
        let p = parts(
            Verdict::Keep,
            PolicyStage::HardProtection,
            Some(CleanupAction::QuarantineMove),
        );
        assert_eq!(
            CleanupCandidate::new(p).err(),
            Some(CandidateError::ActionOnKeep)
        );
        let ok = CleanupCandidate::new(parts(Verdict::Keep, PolicyStage::HardProtection, None));
        assert!(
            ok.is_ok_and(|c| c.is_protected() && c.reversibility() == Reversibility::NotApplicable)
        );
    }

    #[test]
    fn quarantine_needs_a_reversible_action() {
        let classify = PolicyStage::Classification;
        assert_eq!(
            CleanupCandidate::new(parts(Verdict::Quarantine, classify, None)).err(),
            Some(CandidateError::MissingAction)
        );
        for irreversible in [
            tool(),
            CleanupAction::SystemCacheClear,
            CleanupAction::OpenSystemSettings,
        ] {
            assert!(matches!(
                CleanupCandidate::new(parts(Verdict::Quarantine, classify, Some(irreversible))),
                Err(CandidateError::IrreversibleQuarantine { .. })
            ));
        }
        assert!(
            CleanupCandidate::new(parts(
                Verdict::Quarantine,
                classify,
                Some(CleanupAction::QuarantineMove)
            ))
            .is_ok()
        );
        assert!(
            CleanupCandidate::new(parts(
                Verdict::Quarantine,
                classify,
                Some(CleanupAction::MediaTrash)
            ))
            .is_ok()
        );
    }

    #[test]
    fn irreversible_actions_are_allowed_only_under_review() {
        let c = CleanupCandidate::new(parts(
            Verdict::Review,
            PolicyStage::Classification,
            Some(tool()),
        ));
        assert!(
            c.is_ok_and(|c| c.reversibility() == Reversibility::Irreversible && !c.is_protected())
        );
    }

    #[test]
    fn actionability_follows_platform_capabilities() -> Result<(), CandidateError> {
        let c = CleanupCandidate::new(parts(
            Verdict::Quarantine,
            PolicyStage::Classification,
            Some(CleanupAction::QuarantineMove),
        ))?;
        assert!(c.is_actionable_on(&PlatformCapabilities::ceiling(Platform::Macos)));
        assert!(!c.is_actionable_on(&PlatformCapabilities::ceiling(Platform::Ios)));
        Ok(())
    }

    #[test]
    fn deserialization_enforces_action_rules() -> Result<(), Box<dyn std::error::Error>> {
        let c = CleanupCandidate::new(parts(
            Verdict::Review,
            PolicyStage::Classification,
            Some(tool()),
        ))?;
        let json = serde_json::to_string(&c)?;
        assert_eq!(serde_json::from_str::<CleanupCandidate>(&json)?, c);
        let escalated = json.replace("\"verdict\":\"review\"", "\"verdict\":\"quarantine\"");
        assert!(
            serde_json::from_str::<CleanupCandidate>(&escalated).is_err(),
            "irreversible quarantine must be rejected"
        );
        Ok(())
    }
}
