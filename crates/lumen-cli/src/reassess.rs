//! `lumen reassess --db <FILE> [--choose <PATH>]... [--json]`: items of the
//! latest scan checked again with fresh read-only evidence (ADR-0058). Without
//! `--choose`, every item kept or reviewed because of something Lumen did not
//! know is checked. Each item says what it was, what was checked and what it
//! is now; nothing is moved.

use std::path::PathBuf;

use lumen_app::reassess::{Outcome, Reassessed, Reassessment};
use lumen_application::assess::Assessed;
use lumen_application::reassess::{Gap, gaps};
use lumen_domain::Verdict;
use serde_json::{Value, json};

/// Parsed `reassess` arguments.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Args {
    pub(crate) db: PathBuf,
    pub(crate) chosen: Vec<PathBuf>,
    pub(crate) json: bool,
}

pub(crate) fn parse(args: &[String]) -> Result<Args, String> {
    let mut iter = args.iter();
    let mut db = None;
    let mut chosen = Vec::new();
    let mut json = false;
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--json" => json = true,
            "--db" => {
                let path = iter.next().ok_or("--db needs a file path")?;
                if db.replace(PathBuf::from(path)).is_some() {
                    return Err("--db given more than once".into());
                }
            }
            "--choose" => chosen.push(PathBuf::from(
                iter.next().ok_or("--choose needs the path of an item")?,
            )),
            other => return Err(format!("unknown argument: {other}")),
        }
    }
    let db = db.ok_or("reassess needs --db <FILE> [--choose <PATH>]... [--json]")?;
    Ok(Args { db, chosen, json })
}

/// The items to check: the chosen ones, or every one with something unknown.
///
/// # Errors
///
/// A chosen path that is not an item of the latest assessment.
pub(crate) fn select(
    assessed: Vec<Assessed>,
    chosen: &[lumen_domain::RawPath],
) -> Result<Vec<Assessed>, String> {
    if chosen.is_empty() {
        return Ok(assessed
            .into_iter()
            .filter(|item| !gaps(item).is_empty())
            .collect());
    }
    let mut selected = Vec::with_capacity(chosen.len());
    for path in chosen {
        let item = assessed
            .iter()
            .find(|item| item.candidate.path == *path)
            .ok_or_else(|| format!("{} is not an item of the latest assessment", path.display()))?;
        selected.push(item.clone());
    }
    Ok(selected)
}

fn verdict_text(verdict: Verdict) -> &'static str {
    match verdict {
        Verdict::Keep => "KEEP",
        Verdict::Review => "REVIEW",
        Verdict::Quarantine => "QUARANTINE",
    }
}

fn gap_code(gap: Gap) -> &'static str {
    match gap {
        Gap::InUse => "in_use",
        Gap::Unread => "unread",
        Gap::Stale => "stale",
        Gap::Owner => "owner",
    }
}

fn gap_text(gap: Gap) -> &'static str {
    match gap {
        Gap::InUse => "whether a process uses it",
        Gap::Unread => "what is inside it",
        Gap::Stale => "how it is now",
        Gap::Owner => "who owns it and whether that app is installed",
    }
}

/// The report as text, `before` in the same order as `reassessment.items`.
pub(crate) fn to_text(before: &[Assessed], reassessment: &Reassessment) -> String {
    let mut lines = Vec::new();
    let mut changed = 0;
    for (item, earlier) in reassessment.items.iter().zip(before) {
        lines.push(item_path(earlier));
        let was = verdict_text(item.before.verdict);
        match &item.outcome {
            Outcome::Settled => lines.push(format!(
                "  {was}: nothing to check; its verdict does not rest on anything unknown"
            )),
            Outcome::Changed => lines.push(format!(
                "  was {was}; it changed since the scan, so it was not decided again. Scan again."
            )),
            Outcome::Decided { after, added } => {
                let now = verdict_text(after.decision.verdict());
                let checked: Vec<&str> = item.gaps.iter().map(|gap| gap_text(*gap)).collect();
                lines.push(format!("  checked {}", checked.join("; ")));
                if after.decision.verdict() == item.before.verdict {
                    lines.push(format!("  still {now}"));
                } else {
                    changed += 1;
                    lines.push(format!("  was {was}, now {now}"));
                }
                for reason in &after.reasons {
                    lines.push(format!("    - {reason}"));
                }
                lines.push(format!("  {} new fact(s) from the checks", added.len()));
            }
        }
    }
    let done = reassessment.items.len();
    lines.push(format!(
        "{done} item(s) checked again, {changed} with a new verdict (nothing has been moved)"
    ));
    if reassessment.cancelled {
        lines.push("Stopped early: the remaining items were not checked.".into());
    }
    lines.join("\n") + "\n"
}

fn item_path(assessed: &Assessed) -> String {
    assessed.candidate.path.display()
}

fn item_json(item: &Reassessed, earlier: &Assessed) -> Value {
    let rules = |rules: Vec<&str>| rules.into_iter().map(str::to_owned).collect::<Vec<_>>();
    let (outcome, after, added) = match &item.outcome {
        Outcome::Settled => ("settled", Value::Null, 0),
        Outcome::Changed => ("changed", Value::Null, 0),
        Outcome::Decided { after, added } => (
            "decided",
            json!({
                "verdict": verdict_text(after.decision.verdict()),
                "rules": rules(after.decision.fired_rules().iter().map(|f| f.rule.as_str()).collect()),
                "reasons": after.reasons,
                "evidence_hash": after.decision.evidence_hash().to_string(),
            }),
            added.len(),
        ),
    };
    json!({
        "path": item_path(earlier),
        "gaps": item.gaps.iter().map(|gap| gap_code(*gap)).collect::<Vec<_>>(),
        "before": {
            "verdict": verdict_text(item.before.verdict),
            "rules": rules(item.before.rules.iter().map(lumen_domain::RuleId::as_str).collect()),
        },
        "outcome": outcome,
        "after": after,
        "added_facts": added,
    })
}

/// The report as JSON, `before` in the same order as `reassessment.items`.
pub(crate) fn to_json(before: &[Assessed], reassessment: &Reassessment) -> Value {
    json!({
        "items": reassessment
            .items
            .iter()
            .zip(before)
            .map(|(item, earlier)| item_json(item, earlier))
            .collect::<Vec<_>>(),
        "cancelled": reassessment.cancelled,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(args: &[&str]) -> Vec<String> {
        args.iter().map(ToString::to_string).collect()
    }

    #[test]
    fn a_database_is_required_and_items_are_chosen_by_path() {
        assert_eq!(
            parse(&strings(&["--db", "x.db", "--choose", "/a", "--json"])),
            Ok(Args {
                db: "x.db".into(),
                chosen: vec!["/a".into()],
                json: true,
            })
        );
        for args in [
            vec![],
            vec!["--json"],
            vec!["--db"],
            vec!["--db", "x.db", "--choose"],
            vec!["--db", "x.db", "--db", "y.db"],
            vec!["--db", "x.db", "--move"],
        ] {
            assert!(parse(&strings(&args)).is_err(), "{args:?}");
        }
    }
}
