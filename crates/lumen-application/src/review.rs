//! Explain uncertainty without changing the policy decision or its evidence hash.

use std::collections::BTreeSet;

use lumen_domain::{Fact, ReviewReason, Verdict};

use crate::assess::Assessed;

/// Categories supported by this REVIEW decision. Every REVIEW has at least one;
/// KEEP and QUARANTINE have none. Fired rules and their evidence remain authoritative.
pub fn reasons(assessed: &Assessed) -> BTreeSet<ReviewReason> {
    use ReviewReason as R;
    if assessed.decision.verdict() != Verdict::Review {
        return BTreeSet::new();
    }
    let mut reasons = BTreeSet::new();
    let facts: Vec<_> = assessed
        .input
        .evidence
        .iter()
        .filter(|record| record.subject == assessed.input.subject)
        .map(|record| &record.fact)
        .collect();
    if !facts.iter().any(|fact| {
        matches!(
            fact,
            Fact::OwningApplication(_)
                | Fact::KnowledgeBaseMatch { .. }
                | Fact::VendorDeclaredCache { .. }
        )
    }) {
        reasons.insert(R::UnknownOwner);
    }
    if facts.contains(&&Fact::InUse(true)) {
        reasons.insert(R::ActiveProcess);
    }
    if let Some(observed) = &assessed.input.observed {
        if observed.reclaim.partially_linked_files > 0 || observed.entry.size.flags.may_share_blocks
        {
            reasons.insert(R::SharedResource);
        }
        if !observed.reclaim.exact {
            reasons.insert(R::UncertainReclaimableSize);
        }
    } else {
        reasons.insert(R::UncertainReclaimableSize);
    }
    for fired in assessed.decision.fired_rules() {
        match fired.rule.as_str() {
            "classify.devkb-tool-action" => {
                reasons.insert(R::ToolRequired);
            }
            "classify.devkb-not-regenerable" | "classify.app-state" | "classify.orphan" => {
                reasons.insert(R::PossibleUserData);
            }
            "jev.demote" => {
                reasons.insert(R::AiCaution);
            }
            _ => {}
        }
    }
    if reasons.is_empty() {
        reasons.insert(R::Unclassified);
    }
    reasons
}
