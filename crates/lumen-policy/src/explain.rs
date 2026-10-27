//! Explanation templates (roadmap 7.7, ADR-0014).
//!
//! Every fired rule maps to one fixed sentence; [`explain`] renders a decision as
//! those sentences, in the decision's rule order. The text is built only from
//! templates and Lumen's own identifiers, never from untrusted strings (names,
//! labels, model rationale), so it cannot carry injected content.

use lumen_domain::PolicyDecision;

/// The sentence for one rule identifier, or `None` if the rule has no
/// template (a test makes sure every rule has one).
fn template(rule: &str) -> Option<String> {
    if let Some(location) = rule.strip_prefix("protect.location.") {
        return Some(format!("Kept: it is in a protected location ({location})."));
    }
    let text = match rule {
        "default.review" => "Review: Lumen does not recognise this item.",
        "default.keep-small" => "Kept: too small to be worth reviewing.",
        "protect.os-protected" => "Kept: the operating system protects this item.",
        "protect.system-file" => "Kept: Windows marks this item as a system file.",
        "protect.cloud-backed" => {
            "Kept: it is synced with a cloud service, so removing it here could remove it everywhere."
        }
        "protect.in-use" => "Kept: a running process is using it.",
        "protect.in-use-partial" => {
            "Kept: no process Lumen can see uses it, but not every process could be checked."
        }
        "protect.in-use-unknown" => "Kept: Lumen has not checked whether anything uses it.",
        "protect.not-observed" => "Kept: the scan has not observed this item.",
        "protect.incomplete-scan" => {
            "Kept: part of it could not be read, so its contents are unknown."
        }
        "protect.location-data" | "protect.location-context" => {
            "Kept: Lumen could not check its list of protected locations."
        }
        "eligible.quarantine-unsupported" => {
            "Review only: this device cannot quarantine it reversibly."
        }
        "eligible.frees-nothing" => "Kept: removing it would free no space.",
        "eligible.object-kind" => "Review only: it is a link or special file, not data.",
        "eligible.in-use-partial" => {
            "No process Lumen can see uses it; system processes could not be checked, so Lumen checks again just before moving it."
        }
        "eligible.in-use-partial-single-user" => {
            "No process Lumen can see uses it. You are this computer's only user, so the processes that could not be checked are the system's; Lumen checks again just before moving it."
        }
        "classify.devkb-regenerable" => {
            "Quarantine proposed: the tool that made it can regenerate it."
        }
        "classify.devkb-tool-action" => {
            "Review: only the tool's own cleanup command should remove it, and that cannot be undone."
        }
        "classify.devkb-not-regenerable" => "Review: it cannot be regenerated.",
        "classify.devkb-self-managed" => "Kept: the tool that made it cleans it up automatically.",
        "classify.devkb-unknown" => {
            "Review: the knowledge-base entry that matched it is unavailable."
        }
        "classify.vendor-declared-cache" => "Review: its vendor declares its contents disposable.",
        "classify.app-cache" => {
            "Quarantine proposed: it is an application's cache, which the application re-creates."
        }
        "classify.app-state" => {
            "Kept: it holds an application's own data, settings or documents; Lumen leaves it alone."
        }
        "classify.orphan" => "Review: the application it belongs to appears to be uninstalled.",
        "jev.demote" => "Review: the optional AI review advised caution.",
        _ => return None,
    };
    Some(text.to_owned())
}

/// The decision as sentences, one per fired rule. A rule without a template is
/// named rather than hidden.
pub fn explain(decision: &PolicyDecision) -> Vec<String> {
    decision
        .fired_rules()
        .iter()
        .map(|fired| {
            let id = fired.rule.as_str();
            template(id).unwrap_or_else(|| format!("Rule {id} applied."))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use lumen_domain::{Basis, Evidence, Fact, Provenance, SourceName, Timestamp};

    use super::*;
    use crate::testing::{file, input};
    use crate::{KnowledgeClass, PolicyError, evaluate};

    #[test]
    fn every_rule_has_a_template() {
        let ids = crate::rules::RULES.iter().map(|r| r.id).chain([
            "default.review",
            "default.keep-small",
            "jev.demote",
            "protect.location-data",
            "protect.location-context",
        ]);
        for id in ids {
            // `protect.location` itself always fires under a specific id.
            if id != "protect.location" {
                assert!(template(id).is_some(), "{id}");
            }
        }
        assert!(template("protect.location.macos-sip-usr").is_some());
        assert_eq!(template("made.up"), None);
    }

    fn evidence(fact: Fact) -> Evidence {
        Evidence {
            subject: file(1),
            fact,
            basis: Basis::Observed,
            provenance: Provenance {
                source: SourceName::new("test").unwrap_or_else(|_| unreachable!()),
                observed_at: Timestamp::UNIX_EPOCH,
            },
        }
    }

    #[test]
    fn decisions_render_as_fixed_sentences() -> Result<(), PolicyError> {
        let unchecked = evaluate(&input(Vec::new()))?;
        assert_eq!(
            explain(&unchecked),
            ["Kept: Lumen has not checked whether anything uses it."]
        );

        let entry = SourceName::new("cargo-target").unwrap_or_else(|_| unreachable!());
        let mut regenerable = input(vec![
            evidence(Fact::InUse(false)),
            evidence(Fact::KnowledgeBaseMatch {
                entry: entry.clone(),
            }),
        ]);
        regenerable.knowledge = [(entry, KnowledgeClass::Regenerable)].into();
        assert_eq!(
            explain(&evaluate(&regenerable)?),
            ["Quarantine proposed: the tool that made it can regenerate it."]
        );

        let mut system = input(vec![evidence(Fact::InUse(false))]);
        system.context.platform = lumen_domain::Platform::Macos;
        system.paths = vec![
            lumen_domain::RawPath::from_unix_bytes(b"/usr/lib/x".to_vec())
                .unwrap_or_else(|_| unreachable!()),
        ];
        assert_eq!(
            explain(&evaluate(&system)?),
            ["Kept: it is in a protected location (macos-sip-usr)."]
        );
        Ok(())
    }
}
