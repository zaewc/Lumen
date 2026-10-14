//! Classification rules (roadmap 7.5, ADR-0014 stage 3, ADR-0021).
//!
//! Only these rules can make an item a quarantine candidate. They read
//! identification evidence about the subject itself:
//!
//! - a knowledge-base match, judged by the entry's safety class, which the
//!   caller supplies in [`PolicyInput::knowledge`] (the policy never depends on
//!   the knowledge-base crate);
//! - a vendor's cache declaration.

use std::collections::BTreeSet;

use lumen_domain::{Evidence, EvidenceId, Fact, RiskFactor, SourceName, Verdict};

use crate::{Finding, PolicyInput};

/// A knowledge-base entry's safety class (ADR-0021), as the policy sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum KnowledgeClass {
    /// The tool cleans it itself: informational, no action.
    SelfManaged,
    /// Only the tool's own command may clean it; that command is irreversible,
    /// so the item is reviewed individually.
    ToolAction,
    /// The tool regenerates it: a quarantine candidate.
    Regenerable,
    /// It cannot be regenerated: review only.
    NotRegenerable,
}

/// Knowledge-base matches about the subject, by entry.
fn matches(input: &PolicyInput) -> Vec<(&SourceName, EvidenceId)> {
    input
        .evidence
        .iter()
        .filter(|e| e.subject == input.subject)
        .filter_map(|e| match &e.fact {
            Fact::KnowledgeBaseMatch { entry } => Some((entry, e.id())),
            _ => None,
        })
        .collect()
}

/// Proposes `verdict` citing every match whose class satisfies `test`.
fn by_class(
    input: &PolicyInput,
    test: impl Fn(Option<KnowledgeClass>) -> bool,
    verdict: Verdict,
    risk: &[RiskFactor],
) -> Option<Finding> {
    let evidence: BTreeSet<EvidenceId> = matches(input)
        .into_iter()
        .filter(|(entry, _)| test(input.knowledge.get(*entry).copied()))
        .map(|(_, id)| id)
        .collect();
    (!evidence.is_empty()).then(|| Finding {
        verdict,
        evidence,
        risk: risk.iter().copied().collect(),
        rule: None,
    })
}

/// A regenerable artifact, per its knowledge-base entry.
pub(crate) fn devkb_regenerable(input: &PolicyInput) -> Option<Finding> {
    by_class(
        input,
        |c| c == Some(KnowledgeClass::Regenerable),
        Verdict::Quarantine,
        &[],
    )
}

/// Cleaned only by the tool's irreversible command: reviewed individually.
pub(crate) fn devkb_tool_action(input: &PolicyInput) -> Option<Finding> {
    by_class(
        input,
        |c| c == Some(KnowledgeClass::ToolAction),
        Verdict::Review,
        &[],
    )
}

/// Irreplaceable data: review only.
pub(crate) fn devkb_not_regenerable(input: &PolicyInput) -> Option<Finding> {
    by_class(
        input,
        |c| c == Some(KnowledgeClass::NotRegenerable),
        Verdict::Review,
        &[RiskFactor::NotRegenerable],
    )
}

/// The tool manages it: nothing to do.
pub(crate) fn devkb_self_managed(input: &PolicyInput) -> Option<Finding> {
    by_class(
        input,
        |c| c == Some(KnowledgeClass::SelfManaged),
        Verdict::Keep,
        &[],
    )
}

/// A match whose class the caller did not supply: never guessed.
pub(crate) fn devkb_unknown(input: &PolicyInput) -> Option<Finding> {
    by_class(input, |c| c.is_none(), Verdict::Review, &[])
}

/// A vendor declared the folder a disposable cache. The declaration covers the
/// folder's contents, not the folder (e.g. `C:\Windows\Temp`), and candidates
/// cannot yet target contents, so it is reviewed rather than quarantined.
pub(crate) fn vendor_declared_cache(input: &PolicyInput) -> Option<Finding> {
    let evidence: BTreeSet<EvidenceId> = input
        .evidence
        .iter()
        .filter(|e| {
            e.subject == input.subject && matches!(e.fact, Fact::VendorDeclaredCache { .. })
        })
        .map(Evidence::id)
        .collect();
    (!evidence.is_empty()).then(|| Finding {
        verdict: Verdict::Review,
        evidence,
        risk: BTreeSet::new(),
        rule: None,
    })
}

#[cfg(test)]
mod tests {
    use lumen_domain::{Basis, PolicyDecision, Provenance, Subject, Timestamp, UntrustedText};

    use super::*;
    use crate::testing::{file, input};
    use crate::{PolicyError, evaluate};

    fn fact(subject: Subject, fact: Fact) -> Evidence {
        Evidence {
            subject,
            fact,
            basis: Basis::Observed,
            provenance: Provenance {
                source: SourceName::new("devkb").unwrap_or_else(|_| unreachable!()),
                observed_at: Timestamp::UNIX_EPOCH,
            },
        }
    }

    fn entry(id: &str) -> SourceName {
        SourceName::new(id).unwrap_or_else(|_| unreachable!())
    }

    fn kb_match(id: &str) -> Fact {
        Fact::KnowledgeBaseMatch { entry: entry(id) }
    }

    /// Decides a free, observed file with `facts` and the given classes.
    fn decide(
        facts: &[Fact],
        classes: &[(&str, KnowledgeClass)],
    ) -> Result<PolicyDecision, PolicyError> {
        let mut evidence = vec![fact(file(1), Fact::InUse(false))];
        evidence.extend(facts.iter().map(|f| fact(file(1), f.clone())));
        let mut input = input(evidence);
        input.knowledge = classes.iter().map(|(id, c)| (entry(id), *c)).collect();
        evaluate(&input)
    }

    fn fired(decision: &PolicyDecision) -> Vec<&str> {
        decision
            .fired_rules()
            .iter()
            .map(|r| r.rule.as_str())
            .collect()
    }

    #[test]
    fn each_safety_class_gets_its_verdict() -> Result<(), PolicyError> {
        use KnowledgeClass as K;
        for (class, verdict, rule) in [
            (
                K::Regenerable,
                Verdict::Quarantine,
                "classify.devkb-regenerable",
            ),
            (K::ToolAction, Verdict::Review, "classify.devkb-tool-action"),
            (
                K::NotRegenerable,
                Verdict::Review,
                "classify.devkb-not-regenerable",
            ),
            (K::SelfManaged, Verdict::Keep, "classify.devkb-self-managed"),
        ] {
            let decision = decide(&[kb_match("x-cache")], &[("x-cache", class)])?;
            assert_eq!(decision.verdict(), verdict, "{class:?}");
            assert_eq!(fired(&decision), [rule]);
        }
        Ok(())
    }

    #[test]
    fn unknown_classes_and_conflicts_resolve_cautiously() -> Result<(), PolicyError> {
        let unknown = decide(&[kb_match("x-cache")], &[])?;
        assert_eq!(unknown.verdict(), Verdict::Review);
        assert_eq!(fired(&unknown), ["classify.devkb-unknown"]);
        let both = decide(
            &[kb_match("x-cache"), kb_match("y-data")],
            &[
                ("x-cache", KnowledgeClass::Regenerable),
                ("y-data", KnowledgeClass::NotRegenerable),
            ],
        )?;
        assert_eq!(both.verdict(), Verdict::Review, "the most cautious wins");
        assert_eq!(both.risk().level, lumen_domain::RiskLevel::High);
        Ok(())
    }

    #[test]
    fn protections_and_eligibility_still_apply() -> Result<(), PolicyError> {
        let regenerable = [("x-cache", KnowledgeClass::Regenerable)];
        let used = decide(&[kb_match("x-cache"), Fact::InUse(true)], &regenerable)?;
        assert_eq!(used.verdict(), Verdict::Keep);
        let mut input = input(vec![
            fact(file(1), Fact::InUse(false)),
            fact(file(1), kb_match("x-cache")),
        ]);
        input.knowledge = regenerable.iter().map(|(id, c)| (entry(id), *c)).collect();
        input.context.mechanisms.clear();
        assert_eq!(evaluate(&input)?.verdict(), Verdict::Review);
        Ok(())
    }

    #[test]
    fn vendor_declarations_are_reviewed_and_neighbours_ignored() -> Result<(), PolicyError> {
        let declared = Fact::VendorDeclaredCache {
            declared_by: UntrustedText::new("Disk Cleanup handler: Temporary Files"),
        };
        let decision = decide(&[declared], &[])?;
        assert_eq!(decision.verdict(), Verdict::Review);
        assert_eq!(fired(&decision), ["classify.vendor-declared-cache"]);

        let mut input = input(vec![
            fact(file(1), Fact::InUse(false)),
            fact(file(2), kb_match("x-cache")),
        ]);
        input.knowledge = [(entry("x-cache"), KnowledgeClass::Regenerable)].into();
        assert_eq!(
            fired(&evaluate(&input)?),
            ["default.review"],
            "a neighbour's match says nothing about this item"
        );
        Ok(())
    }
}
