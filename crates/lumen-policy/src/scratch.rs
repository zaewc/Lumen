//! User logs and temporary files (ADR-0070, roadmap 15.11).
//!
//! Each item directly inside the user's log folder (macOS `~/Library/Logs`) or
//! temporary folder (Windows) carries [`Fact::ScratchLocation`]. Its location
//! makes it a review item a person may choose (ADR-0051); nothing here ever
//! proposes quarantine. Age only adds caution (ADR-0059): an item that changed
//! recently, whose age is unknown, or that is or holds a crash report, is kept.
//!
//! Age is `as_of - at` of a [`Fact::NewestChange`], never the clock, so a
//! decision replays exactly from its evidence.

use std::collections::BTreeSet;

use lumen_domain::{Evidence, EvidenceId, Fact, RiskFactor, ScratchKind, Verdict};

use crate::{Finding, PolicyInput};

/// Temporary files that changed within this many days are kept: Disk
/// Cleanup's `Temporary Files` handler defaults `LastAccess` to 7 days, and
/// Microsoft says temporary files not modified "in over a week" may go.
pub const TEMPORARY_KEEP_DAYS: i64 = 7;

/// Logs that changed within this many days are kept. Lumen's own choice: no
/// Apple source gives an age, and a log can be the only record of a problem
/// that shows up later.
pub const LOGS_KEEP_DAYS: i64 = 30;

/// How long an item of `kind` stays kept after its newest change.
fn keep_days(kind: ScratchKind) -> i64 {
    match kind {
        ScratchKind::Temporary => TEMPORARY_KEEP_DAYS,
        // Logs, and any kind this policy does not know, get the longer age.
        _ => LOGS_KEEP_DAYS,
    }
}

/// Facts about the subject itself that satisfy `test`.
fn about<'a>(
    input: &'a PolicyInput,
    test: impl Fn(&Fact) -> bool + 'a,
) -> impl Iterator<Item = &'a Evidence> + 'a {
    input
        .evidence
        .iter()
        .filter(move |e| e.subject == input.subject && test(&e.fact))
}

fn ids<'a>(evidence: impl Iterator<Item = &'a Evidence>) -> BTreeSet<EvidenceId> {
    evidence.map(Evidence::id).collect()
}

fn locations(input: &PolicyInput) -> BTreeSet<EvidenceId> {
    ids(about(input, |f| matches!(f, Fact::ScratchLocation { .. })))
}

/// The longest keep period of the subject's scratch kinds, if it has any.
fn longest_keep(input: &PolicyInput) -> Option<i64> {
    about(input, |f| matches!(f, Fact::ScratchLocation { .. }))
        .filter_map(|e| match e.fact {
            Fact::ScratchLocation { kind } => Some(keep_days(kind)),
            _ => None,
        })
        .max()
}

fn finding(verdict: Verdict, evidence: BTreeSet<EvidenceId>, risk: &[RiskFactor]) -> Finding {
    Finding {
        verdict,
        evidence,
        risk: risk.iter().copied().collect(),
        rule: None,
    }
}

/// An item in the user's log or temporary folder: a person may choose it.
pub(crate) fn scratch(input: &PolicyInput) -> Option<Finding> {
    let evidence = locations(input);
    (!evidence.is_empty()).then(|| finding(Verdict::Review, evidence, &[]))
}

/// The item changed within its keep period, or a change time is later than
/// the observation that recorded it (a clock or timestamp Lumen cannot trust).
pub(crate) fn scratch_recent(input: &PolicyInput) -> Option<Finding> {
    let days = longest_keep(input)?;
    let recent: BTreeSet<EvidenceId> = ids(about(
        input,
        move |f| matches!(f, Fact::NewestChange { at, as_of } if as_of.whole_days_since(*at) < days || at > as_of),
    ));
    (!recent.is_empty()).then(|| {
        let evidence = locations(input).union(&recent).copied().collect();
        finding(Verdict::Keep, evidence, &[])
    })
}

/// No complete reading of when the item last changed: unknown age is never
/// old (ADR-0008).
pub(crate) fn scratch_age_unknown(input: &PolicyInput) -> Option<Finding> {
    longest_keep(input)?;
    let dated = about(input, |f| matches!(f, Fact::NewestChange { .. })).next();
    dated.is_none().then(|| {
        finding(
            Verdict::Keep,
            locations(input),
            &[RiskFactor::IncompleteCoverage],
        )
    })
}

/// The item is, or holds, a crash, spin or diagnostic report a person may
/// need, wherever it lies.
pub(crate) fn scratch_report(input: &PolicyInput) -> Option<Finding> {
    let evidence = ids(about(input, |f| matches!(f, Fact::DiagnosticReport { .. })));
    (!evidence.is_empty()).then(|| finding(Verdict::Keep, evidence, &[RiskFactor::NotRegenerable]))
}

#[cfg(test)]
mod tests {
    use lumen_domain::{
        Basis, PolicyDecision, Provenance, SourceName, Subject, Timestamp, UntrustedText,
    };

    use super::*;
    use crate::testing::{file, input};
    use crate::{PolicyError, evaluate};

    const DAY: i64 = 86_400;
    const NOW: i64 = 1_800_000_000;

    fn record(subject: Subject, fact: Fact) -> Evidence {
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

    fn at(secs: i64) -> Timestamp {
        Timestamp::from_unix(secs, 0).unwrap_or_else(|_| unreachable!())
    }

    fn location(kind: ScratchKind) -> Fact {
        Fact::ScratchLocation { kind }
    }

    /// Changed `days` days (plus `extra` seconds) before the observation.
    fn changed(days: i64, extra: i64) -> Fact {
        Fact::NewestChange {
            at: at(NOW - days * DAY - extra),
            as_of: at(NOW),
        }
    }

    fn report() -> Fact {
        Fact::DiagnosticReport {
            marker: UntrustedText::new("App/crash.ips"),
        }
    }

    fn decide(facts: &[Fact]) -> Result<PolicyDecision, PolicyError> {
        let mut evidence = vec![record(file(1), Fact::InUse(false))];
        evidence.extend(facts.iter().map(|f| record(file(1), f.clone())));
        evaluate(&input(evidence))
    }

    fn fired(decision: &PolicyDecision) -> Vec<&str> {
        decision
            .fired_rules()
            .iter()
            .map(|r| r.rule.as_str())
            .collect()
    }

    #[test]
    fn old_items_are_reviewed_never_proposed() -> Result<(), PolicyError> {
        for (kind, days) in [
            (ScratchKind::Temporary, TEMPORARY_KEEP_DAYS),
            (ScratchKind::Logs, LOGS_KEEP_DAYS),
        ] {
            let decision = decide(&[location(kind), changed(days, 0)])?;
            assert_eq!(decision.verdict(), Verdict::Review, "{kind:?}");
            assert_eq!(fired(&decision), ["classify.scratch"], "{kind:?}");
        }
        Ok(())
    }

    #[test]
    fn recent_and_future_changes_are_kept() -> Result<(), PolicyError> {
        for (kind, days) in [
            (ScratchKind::Temporary, TEMPORARY_KEEP_DAYS),
            (ScratchKind::Logs, LOGS_KEEP_DAYS),
        ] {
            // One second short of the keep period is still recent.
            let recent = decide(&[location(kind), changed(days - 1, DAY - 1)])?;
            assert_eq!(recent.verdict(), Verdict::Keep, "{kind:?}");
            assert_eq!(
                fired(&recent),
                ["classify.scratch", "classify.scratch-recent"]
            );
            let future = decide(&[location(kind), changed(-1, 0)])?;
            assert_eq!(future.verdict(), Verdict::Keep, "{kind:?}");
        }
        // A 10-day-old log is recent; a 10-day-old temporary file is not.
        let log = decide(&[location(ScratchKind::Logs), changed(10, 0)])?;
        assert_eq!(log.verdict(), Verdict::Keep);
        let temp = decide(&[location(ScratchKind::Temporary), changed(10, 0)])?;
        assert_eq!(temp.verdict(), Verdict::Review);
        Ok(())
    }

    #[test]
    fn any_recent_reading_keeps() -> Result<(), PolicyError> {
        let decision = decide(&[
            location(ScratchKind::Temporary),
            changed(90, 0),
            changed(1, 0),
        ])?;
        assert_eq!(decision.verdict(), Verdict::Keep);
        // Two kinds: the longer keep period applies.
        let both = decide(&[
            location(ScratchKind::Temporary),
            location(ScratchKind::Logs),
            changed(10, 0),
        ])?;
        assert_eq!(both.verdict(), Verdict::Keep);
        Ok(())
    }

    #[test]
    fn unknown_age_is_kept() -> Result<(), PolicyError> {
        let decision = decide(&[location(ScratchKind::Temporary)])?;
        assert_eq!(decision.verdict(), Verdict::Keep);
        assert_eq!(
            fired(&decision),
            ["classify.scratch", "classify.scratch-age-unknown"]
        );
        Ok(())
    }

    #[test]
    fn reports_and_folders_holding_them_are_kept() -> Result<(), PolicyError> {
        let decision = decide(&[location(ScratchKind::Logs), changed(400, 0), report()])?;
        assert_eq!(decision.verdict(), Verdict::Keep);
        assert_eq!(
            fired(&decision),
            ["classify.scratch", "classify.scratch-report"]
        );
        // A report is kept wherever it is.
        assert_eq!(decide(&[report()])?.verdict(), Verdict::Keep);
        Ok(())
    }

    #[test]
    fn scratch_never_lifts_another_rules_caution() -> Result<(), PolicyError> {
        let used = decide(&[
            Fact::InUse(true),
            location(ScratchKind::Temporary),
            changed(400, 0),
        ])?;
        assert_eq!(used.verdict(), Verdict::Keep);
        assert_eq!(fired(&used), ["protect.in-use"]);
        let cloud = decide(&[
            Fact::CloudBacked(true),
            location(ScratchKind::Logs),
            changed(400, 0),
        ])?;
        assert_eq!(cloud.verdict(), Verdict::Keep);
        Ok(())
    }

    #[test]
    fn facts_about_a_neighbour_are_ignored() -> Result<(), PolicyError> {
        let mut evidence = vec![
            record(file(1), Fact::InUse(false)),
            record(file(1), location(ScratchKind::Temporary)),
            record(file(1), changed(400, 0)),
        ];
        evidence.push(record(file(2), changed(1, 0)));
        evidence.push(record(file(2), report()));
        let decision = evaluate(&input(evidence))?;
        assert_eq!(decision.verdict(), Verdict::Review);
        Ok(())
    }
}
