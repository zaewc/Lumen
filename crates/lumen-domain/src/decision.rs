//! Verdicts, risk and policy decisions.
//!
//! The deterministic safety policy (ADR-0014) produces a [`PolicyDecision`] for
//! every candidate. Its invariants are enforced here, in the type, so that no
//! code path (policy bug, deserialized record, UI) can hold a decision that, for
//! example, quarantines an item a hard protection said to keep.

use std::collections::BTreeSet;
use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::name::is_valid_name;
use crate::{EvidenceHash, EvidenceId, PolicyVersion};

/// The final outcome for a candidate. There is deliberately no "remove":
/// permanent deletion only happens when quarantine is finalized (ADR-0015).
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
pub enum Verdict {
    /// Leave the item alone.
    Keep,
    /// Show the evidence; act only on the user's individual decision.
    Review,
    /// Propose a reversible quarantine; still requires plan confirmation.
    Quarantine,
}

/// How much harm removing an item could do.
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
pub enum RiskLevel {
    /// Regenerable data; no user impact beyond regeneration time.
    Low,
    /// Some inconvenience (sign-ins, re-downloads, rebuilds).
    Medium,
    /// Possible data loss or broken software.
    High,
    /// System, security or credential impact.
    Critical,
}

/// Why an item carries risk.
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
pub enum RiskFactor {
    /// Runs or is installed system-wide.
    SystemScope,
    /// No valid code signature.
    Unsigned,
    /// Owning application unknown.
    UnknownOwner,
    /// In use by a running process.
    InUse,
    /// Cannot be regenerated.
    NotRegenerable,
    /// Backed by a cloud provider; removal may propagate.
    CloudBacked,
    /// Some relevant location or source was not fully observed.
    IncompleteCoverage,
    /// Protected by the operating system.
    OsProtected,
    /// Looks like credentials, keys, a database or application state.
    SensitiveData,
}

/// Risk assessment: a level and the factors behind it.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Risk {
    /// Overall level.
    pub level: RiskLevel,
    /// Contributing factors.
    pub factors: BTreeSet<RiskFactor>,
}

/// Stable identifier of a policy rule, e.g. `protect.sip` or
/// `classify.app-cache`: 1–64 bytes of `[a-z0-9.-]` starting with a letter.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RuleId(String);

/// Error returned for an invalid [`RuleId`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("rule ID must be 1-64 bytes of [a-z0-9.-] starting with a letter")]
pub struct RuleIdError;

impl RuleId {
    /// Validates and wraps a rule identifier.
    ///
    /// # Errors
    ///
    /// Returns [`RuleIdError`] if the identifier is invalid.
    pub fn new(id: impl Into<String>) -> Result<Self, RuleIdError> {
        let id = id.into();
        if is_valid_name(&id) {
            Ok(Self(id))
        } else {
            Err(RuleIdError)
        }
    }

    /// The identifier.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for RuleId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for RuleId {
    type Err = RuleIdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new(s)
    }
}

impl Serialize for RuleId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for RuleId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Self::new(text).map_err(serde::de::Error::custom)
    }
}

/// Policy evaluation stage (ADR-0014), in evaluation order.
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
pub enum PolicyStage {
    /// Non-overridable protections; any rule here forces `Keep`.
    HardProtection,
    /// Platform, volume and reclaimability checks.
    Eligibility,
    /// Category rules producing a candidate verdict.
    Classification,
    /// Optional Jev evidence (monotone: may only add caution by default).
    Jev,
}

/// A rule that fired, with the evidence it used.
#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, schemars::JsonSchema,
)]
pub struct FiredRule {
    /// Rule identifier.
    pub rule: RuleId,
    /// Stage the rule belongs to.
    pub stage: PolicyStage,
    /// Evidence the rule relied on.
    pub evidence: BTreeSet<EvidenceId>,
}

/// What Jev evidence did to the decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum JevEffect {
    /// Jev was not consulted, unavailable, or changed nothing.
    None,
    /// Jev evidence made the decision more cautious (`Quarantine` → `Review`).
    DemotedToReview,
    /// Jev evidence promoted `Review` → `Quarantine` under an allow-listed,
    /// calibrated class rule (off by default; ADR-0020).
    Promoted,
}

/// Error returned when a decision violates the policy contract.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum DecisionError {
    /// A decision must cite at least one fired rule.
    #[error("a policy decision must cite at least one fired rule")]
    NoRules,
    /// A hard protection fired but the verdict is not `Keep`.
    #[error("a hard-protection rule fired, so the verdict must be keep (got {verdict:?})")]
    ProtectionOverridden {
        /// The offending verdict.
        verdict: Verdict,
    },
    /// The Jev effect does not match the verdict.
    #[error("Jev effect {effect:?} is inconsistent with verdict {verdict:?}")]
    InconsistentJevEffect {
        /// Recorded effect.
        effect: JevEffect,
        /// Verdict.
        verdict: Verdict,
    },
}

/// The deterministic policy's decision for one candidate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, schemars::JsonSchema)]
pub struct PolicyDecision {
    verdict: Verdict,
    policy_version: PolicyVersion,
    evidence_hash: EvidenceHash,
    fired_rules: BTreeSet<FiredRule>,
    risk: Risk,
    jev_effect: JevEffect,
}

impl PolicyDecision {
    /// Builds a decision, enforcing the policy contract:
    ///
    /// - at least one rule fired;
    /// - if any hard-protection rule fired, the verdict is `Keep` and Jev had no
    ///   effect;
    /// - `DemotedToReview` implies `Review`; `Promoted` implies `Quarantine`.
    ///
    /// # Errors
    ///
    /// Returns a [`DecisionError`] describing the violated rule.
    pub fn new(
        verdict: Verdict,
        policy_version: PolicyVersion,
        evidence_hash: EvidenceHash,
        fired_rules: impl IntoIterator<Item = FiredRule>,
        risk: Risk,
        jev_effect: JevEffect,
    ) -> Result<Self, DecisionError> {
        let fired_rules: BTreeSet<_> = fired_rules.into_iter().collect();
        if fired_rules.is_empty() {
            return Err(DecisionError::NoRules);
        }
        let protected = fired_rules
            .iter()
            .any(|r| r.stage == PolicyStage::HardProtection);
        if protected && verdict != Verdict::Keep {
            return Err(DecisionError::ProtectionOverridden { verdict });
        }
        let consistent = match jev_effect {
            JevEffect::None => true,
            JevEffect::DemotedToReview => verdict == Verdict::Review && !protected,
            JevEffect::Promoted => verdict == Verdict::Quarantine && !protected,
        };
        if !consistent {
            return Err(DecisionError::InconsistentJevEffect {
                effect: jev_effect,
                verdict,
            });
        }
        Ok(Self {
            verdict,
            policy_version,
            evidence_hash,
            fired_rules,
            risk,
            jev_effect,
        })
    }

    /// The verdict.
    pub const fn verdict(&self) -> Verdict {
        self.verdict
    }

    /// Policy version that produced the decision.
    pub const fn policy_version(&self) -> PolicyVersion {
        self.policy_version
    }

    /// Digest of the evidence bundle the decision used.
    pub const fn evidence_hash(&self) -> EvidenceHash {
        self.evidence_hash
    }

    /// Rules that fired, with their evidence.
    pub fn fired_rules(&self) -> &BTreeSet<FiredRule> {
        &self.fired_rules
    }

    /// Risk assessment.
    pub fn risk(&self) -> &Risk {
        &self.risk
    }

    /// Effect of Jev evidence.
    pub const fn jev_effect(&self) -> JevEffect {
        self.jev_effect
    }
}

impl<'de> Deserialize<'de> for PolicyDecision {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Wire {
            verdict: Verdict,
            policy_version: PolicyVersion,
            evidence_hash: EvidenceHash,
            fired_rules: BTreeSet<FiredRule>,
            risk: Risk,
            jev_effect: JevEffect,
        }
        let w = Wire::deserialize(deserializer)?;
        Self::new(
            w.verdict,
            w.policy_version,
            w.evidence_hash,
            w.fired_rules,
            w.risk,
            w.jev_effect,
        )
        .map_err(serde::de::Error::custom)
    }
}

manual_schema!(RuleId, "RuleId", { "type": "string", "pattern": concat!("^", name_regex!(), "$") });

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn rule(id: &str, stage: PolicyStage) -> FiredRule {
        FiredRule {
            rule: RuleId::new(id).unwrap_or_else(|_| unreachable!()),
            stage,
            evidence: BTreeSet::new(),
        }
    }

    fn low_risk() -> Risk {
        Risk {
            level: RiskLevel::Low,
            factors: BTreeSet::new(),
        }
    }

    fn decide(
        verdict: Verdict,
        rules: Vec<FiredRule>,
        jev: JevEffect,
    ) -> Result<PolicyDecision, DecisionError> {
        PolicyDecision::new(
            verdict,
            PolicyVersion::new(1, 0, 0),
            EvidenceHash::from_bytes([7; 32]),
            rules,
            low_risk(),
            jev,
        )
    }

    #[test]
    fn hard_protection_forces_keep() {
        let sip = rule("protect.sip", PolicyStage::HardProtection);
        assert!(decide(Verdict::Keep, vec![sip.clone()], JevEffect::None).is_ok());
        for verdict in [Verdict::Review, Verdict::Quarantine] {
            assert_eq!(
                decide(
                    verdict,
                    vec![
                        sip.clone(),
                        rule("classify.app-cache", PolicyStage::Classification)
                    ],
                    JevEffect::None
                ),
                Err(DecisionError::ProtectionOverridden { verdict })
            );
        }
    }

    #[test]
    fn jev_cannot_touch_protected_items() {
        let sip = rule("protect.sip", PolicyStage::HardProtection);
        assert!(decide(Verdict::Keep, vec![sip], JevEffect::Promoted).is_err());
    }

    #[test]
    fn jev_effects_must_match_verdicts() {
        let cache = rule("classify.app-cache", PolicyStage::Classification);
        assert!(
            decide(
                Verdict::Review,
                vec![cache.clone()],
                JevEffect::DemotedToReview
            )
            .is_ok()
        );
        assert!(
            decide(
                Verdict::Quarantine,
                vec![cache.clone()],
                JevEffect::DemotedToReview
            )
            .is_err()
        );
        assert!(
            decide(
                Verdict::Quarantine,
                vec![cache.clone()],
                JevEffect::Promoted
            )
            .is_ok()
        );
        assert!(decide(Verdict::Review, vec![cache], JevEffect::Promoted).is_err());
    }

    #[test]
    fn decisions_need_rules() {
        assert_eq!(
            decide(Verdict::Keep, vec![], JevEffect::None),
            Err(DecisionError::NoRules)
        );
    }

    #[test]
    fn deserialization_enforces_the_contract() -> serde_json::Result<()> {
        let ok = decide(
            Verdict::Keep,
            vec![rule("protect.sip", PolicyStage::HardProtection)],
            JevEffect::None,
        )
        .unwrap_or_else(|_| unreachable!());
        let json = serde_json::to_string(&ok)?;
        assert_eq!(serde_json::from_str::<PolicyDecision>(&json)?, ok);
        let tampered = json.replace("\"verdict\":\"keep\"", "\"verdict\":\"quarantine\"");
        assert!(
            serde_json::from_str::<PolicyDecision>(&tampered).is_err(),
            "tampered verdict must be rejected"
        );
        Ok(())
    }

    fn arb_stage() -> impl Strategy<Value = PolicyStage> {
        prop_oneof![
            Just(PolicyStage::HardProtection),
            Just(PolicyStage::Eligibility),
            Just(PolicyStage::Classification),
            Just(PolicyStage::Jev),
        ]
    }

    fn arb_verdict() -> impl Strategy<Value = Verdict> {
        prop_oneof![
            Just(Verdict::Keep),
            Just(Verdict::Review),
            Just(Verdict::Quarantine)
        ]
    }

    fn arb_jev() -> impl Strategy<Value = JevEffect> {
        prop_oneof![
            Just(JevEffect::None),
            Just(JevEffect::DemotedToReview),
            Just(JevEffect::Promoted)
        ]
    }

    proptest! {
        #[test]
        fn no_accepted_decision_acts_on_a_protected_item(
            stages in prop::collection::vec(arb_stage(), 0..6),
            verdict in arb_verdict(),
            jev in arb_jev(),
        ) {
            let rules: Vec<_> = stages.iter().enumerate().map(|(i, s)| rule(&format!("r{i}"), *s)).collect();
            if let Ok(decision) = decide(verdict, rules, jev) {
                let protected = stages.contains(&PolicyStage::HardProtection);
                prop_assert!(!protected || (decision.verdict() == Verdict::Keep && decision.jev_effect() == JevEffect::None));
                prop_assert!(decision.jev_effect() != JevEffect::DemotedToReview || decision.verdict() == Verdict::Review);
            }
        }
    }
}
