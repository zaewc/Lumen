//! The policy's rules, in evaluation order within each stage.
//!
//! Rules read only evidence about the subject itself; facts about neighbours
//! reach a decision through the relationships that cite them.

use std::collections::BTreeSet;

use lumen_domain::{Evidence, EvidenceId, Fact, PolicyStage, RiskFactor, Verdict};

use crate::{Finding, PolicyInput, Rule};

/// Every rule the policy evaluates.
pub(crate) const RULES: &[Rule] = &[
    Rule {
        id: "protect.location",
        stage: PolicyStage::HardProtection,
        check: crate::locations::check,
    },
    Rule {
        id: "protect.os-protected",
        stage: PolicyStage::HardProtection,
        check: os_protected,
    },
    Rule {
        id: "protect.system-file",
        stage: PolicyStage::HardProtection,
        check: system_file,
    },
    Rule {
        id: "protect.cloud-backed",
        stage: PolicyStage::HardProtection,
        check: cloud_backed,
    },
    Rule {
        id: "protect.in-use",
        stage: PolicyStage::HardProtection,
        check: in_use,
    },
    Rule {
        id: "protect.in-use-partial",
        stage: PolicyStage::HardProtection,
        check: in_use_partial,
    },
    Rule {
        id: "protect.in-use-unknown",
        stage: PolicyStage::HardProtection,
        check: in_use_unknown,
    },
];

/// Evidence about the subject itself whose fact satisfies `test`.
fn about(input: &PolicyInput, test: impl Fn(&Fact) -> bool) -> BTreeSet<EvidenceId> {
    input
        .evidence
        .iter()
        .filter(|e| e.subject == input.subject && test(&e.fact))
        .map(Evidence::id)
        .collect()
}

/// A protection citing `evidence`, if there is any.
fn keep_if(evidence: BTreeSet<EvidenceId>, risk: RiskFactor) -> Option<Finding> {
    (!evidence.is_empty()).then(|| Finding {
        verdict: Verdict::Keep,
        evidence,
        risk: BTreeSet::from([risk]),
        rule: None,
    })
}

/// The operating system forbids removal (SIP / `SF_RESTRICTED`, `SF_NOUNLINK`,
/// immutable flags).
fn os_protected(input: &PolicyInput) -> Option<Finding> {
    keep_if(
        about(
            input,
            |f| matches!(f, Fact::Protected(p) if p.os_protected()),
        ),
        RiskFactor::OsProtected,
    )
}

/// Windows marks the file as part of the operating system
/// (`FILE_ATTRIBUTE_SYSTEM`).
fn system_file(input: &PolicyInput) -> Option<Finding> {
    keep_if(
        about(
            input,
            |f| matches!(f, Fact::Protected(p) if p.system_attribute),
        ),
        RiskFactor::OsProtected,
    )
}

/// Cloud placeholders and synced items: removal may propagate to every device.
fn cloud_backed(input: &PolicyInput) -> Option<Finding> {
    keep_if(
        about(input, |f| *f == Fact::CloudBacked(true)),
        RiskFactor::CloudBacked,
    )
}

/// A process uses the item.
fn in_use(input: &PolicyInput) -> Option<Finding> {
    keep_if(about(input, |f| *f == Fact::InUse(true)), RiskFactor::InUse)
}

fn complete_no(input: &PolicyInput) -> bool {
    !about(input, |f| *f == Fact::InUse(false)).is_empty()
}

/// Only some processes could be checked. ADR-0030 allows this for the user's
/// own files under narrow conditions; until the policy can verify them, a
/// partial answer keeps the item.
fn in_use_partial(input: &PolicyInput) -> Option<Finding> {
    if complete_no(input) || in_use(input).is_some() {
        return None;
    }
    keep_if(
        about(input, |f| {
            matches!(f, Fact::NotInUseByVisibleProcesses { .. })
        }),
        RiskFactor::IncompleteCoverage,
    )
}

/// Nobody checked whether the item is in use: unknown is never "unused".
fn in_use_unknown(input: &PolicyInput) -> Option<Finding> {
    let answered = about(input, |f| {
        matches!(f, Fact::InUse(_) | Fact::NotInUseByVisibleProcesses { .. })
    });
    answered.is_empty().then(|| Finding {
        verdict: Verdict::Keep,
        evidence: BTreeSet::new(),
        risk: BTreeSet::from([RiskFactor::IncompleteCoverage]),
        rule: None,
    })
}

#[cfg(test)]
mod tests {
    use lumen_domain::{
        Basis, EvidenceHash, FileId, FileIdentity, PolicyDecision, Protection, Provenance,
        RiskLevel, SourceName, Subject, Timestamp, VolumeId,
    };

    use super::*;
    use crate::{PolicyError, evaluate};

    fn file(n: u128) -> Subject {
        Subject::File {
            identity: FileIdentity {
                volume: VolumeId::new("v").unwrap_or_else(|_| unreachable!()),
                file: FileId::new(n),
            },
        }
    }

    fn fact(subject: Subject, fact: Fact) -> Evidence {
        Evidence {
            subject,
            fact,
            basis: Basis::Observed,
            provenance: Provenance {
                source: SourceName::new("test").unwrap_or_else(|_| unreachable!()),
                observed_at: Timestamp::UNIX_EPOCH,
            },
        }
    }

    fn decide(facts: &[Fact]) -> Result<PolicyDecision, PolicyError> {
        evaluate(&PolicyInput {
            subject: file(1),
            evidence_hash: EvidenceHash::from_bytes([0; 32]),
            paths: Vec::new(),
            context: crate::Context {
                platform: lumen_domain::Platform::Linux,
                home: Some(
                    lumen_domain::RawPath::from_unix_bytes(b"/home/t".to_vec())
                        .unwrap_or_else(|_| unreachable!()),
                ),
                windows_dir: None,
                local_app_data: None,
            },
            evidence: facts.iter().map(|f| fact(file(1), f.clone())).collect(),
        })
    }

    fn fired(decision: &PolicyDecision) -> Vec<&str> {
        decision
            .fired_rules()
            .iter()
            .map(|r| r.rule.as_str())
            .collect()
    }

    const NOT_IN_USE: Fact = Fact::InUse(false);

    #[test]
    fn an_unprotected_idle_item_reaches_classification() -> Result<(), PolicyError> {
        let decision = decide(&[NOT_IN_USE])?;
        assert_eq!(decision.verdict(), Verdict::Review);
        assert_eq!(fired(&decision), ["default.review"]);
        Ok(())
    }

    #[test]
    fn observed_protections_keep() -> Result<(), PolicyError> {
        let sip = Protection {
            system_restricted: true,
            ..Protection::default()
        };
        let immutable = Protection {
            undeletable: true,
            ..Protection::default()
        };
        let system = Protection {
            system_attribute: true,
            ..Protection::default()
        };
        for (protection, rule) in [
            (sip, "protect.os-protected"),
            (immutable, "protect.os-protected"),
            (system, "protect.system-file"),
        ] {
            let decision = decide(&[NOT_IN_USE, Fact::Protected(protection)])?;
            assert_eq!(decision.verdict(), Verdict::Keep);
            assert_eq!(fired(&decision), [rule]);
            assert_eq!(decision.risk().level, RiskLevel::Critical);
        }
        let hidden = Protection {
            hidden: true,
            ..Protection::default()
        };
        assert_eq!(
            decide(&[NOT_IN_USE, Fact::Protected(hidden)])?.verdict(),
            Verdict::Review,
            "hidden alone is not a protection"
        );
        let cloud = decide(&[NOT_IN_USE, Fact::CloudBacked(true)])?;
        assert_eq!(fired(&cloud), ["protect.cloud-backed"]);
        assert_eq!(cloud.verdict(), Verdict::Keep);
        Ok(())
    }

    #[test]
    fn only_a_complete_no_passes_the_in_use_gate() -> Result<(), PolicyError> {
        let used = decide(&[Fact::InUse(true)])?;
        assert_eq!(fired(&used), ["protect.in-use"]);
        let partial = decide(&[Fact::NotInUseByVisibleProcesses { unseen: Some(2) }])?;
        assert_eq!(fired(&partial), ["protect.in-use-partial"]);
        assert_eq!(partial.verdict(), Verdict::Keep);
        let unknown = decide(&[])?;
        assert_eq!(fired(&unknown), ["protect.in-use-unknown"]);
        assert_eq!(unknown.verdict(), Verdict::Keep);
        let complete = decide(&[
            Fact::NotInUseByVisibleProcesses { unseen: None },
            NOT_IN_USE,
        ])?;
        assert_eq!(complete.verdict(), Verdict::Review, "a complete no wins");
        let conflicting = decide(&[NOT_IN_USE, Fact::InUse(true)])?;
        assert_eq!(fired(&conflicting), ["protect.in-use"], "any yes keeps");
        Ok(())
    }

    #[test]
    fn evidence_about_other_subjects_is_ignored() -> Result<(), PolicyError> {
        let decision = evaluate(&PolicyInput {
            subject: file(1),
            evidence_hash: EvidenceHash::from_bytes([0; 32]),
            paths: Vec::new(),
            context: crate::Context {
                platform: lumen_domain::Platform::Linux,
                home: Some(
                    lumen_domain::RawPath::from_unix_bytes(b"/home/t".to_vec())
                        .unwrap_or_else(|_| unreachable!()),
                ),
                windows_dir: None,
                local_app_data: None,
            },
            evidence: vec![
                fact(file(1), NOT_IN_USE),
                fact(file(2), Fact::InUse(true)),
                fact(file(2), Fact::CloudBacked(true)),
            ],
        })?;
        assert_eq!(decision.verdict(), Verdict::Review);
        Ok(())
    }

    #[test]
    fn protections_cite_their_evidence() -> Result<(), PolicyError> {
        let used = fact(file(1), Fact::InUse(true));
        let decision = evaluate(&PolicyInput {
            subject: file(1),
            evidence_hash: EvidenceHash::from_bytes([0; 32]),
            paths: Vec::new(),
            context: crate::Context {
                platform: lumen_domain::Platform::Linux,
                home: Some(
                    lumen_domain::RawPath::from_unix_bytes(b"/home/t".to_vec())
                        .unwrap_or_else(|_| unreachable!()),
                ),
                windows_dir: None,
                local_app_data: None,
            },
            evidence: vec![used.clone()],
        })?;
        let rule = decision
            .fired_rules()
            .first()
            .ok_or_else(|| PolicyError::RuleId("none".into()))?;
        assert_eq!(rule.evidence, BTreeSet::from([used.id()]));
        Ok(())
    }
}
