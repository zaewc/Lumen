//! `lumen assess`: the latest completed scan's candidates with their
//! verdicts and reasons, as text or JSON (roadmap 10.13b). The assessment
//! itself is `lumen_app::assess`. With AI review on, review items may carry
//! an AI note on what they likely are (ADR-0041), shown labelled as AI.
//! Items kept only because running apps use them say which apps to quit to
//! clean them up on the next scan; their verdict stays `KEEP`.

use lumen_app::assess::{ToolGuidance, tool_action};
use lumen_application::assess::{Assessed, FreeWhenClosed};
use lumen_application::describe::{MAX_DESCRIBED, describable};
use lumen_domain::{ByteCount, ItemKind, ItemNote, JudgmentConfidence, Verdict};
use serde_json::{Value, json};

use crate::report::human;

fn verdict_text(verdict: Verdict) -> &'static str {
    match verdict {
        Verdict::Keep => "KEEP",
        Verdict::Review => "REVIEW",
        Verdict::Quarantine => "QUARANTINE",
    }
}

fn reclaim(assessed: &Assessed) -> ByteCount {
    assessed
        .input
        .observed
        .as_ref()
        .map_or(ByteCount::ZERO, |o| o.reclaim.reclaimable_now)
}

/// One shell word: as is when it is plainly safe, otherwise single-quoted
/// (with each `'` written as `'\''`), so a copied command means what it shows.
fn shell_word(word: &str) -> String {
    let plain = !word.is_empty()
        && word
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_./:=@%+,".contains(&b));
    if plain {
        word.to_owned()
    } else {
        format!("'{}'", word.replace('\'', "'\\''"))
    }
}

/// The tool's command as one line to copy, each word quoted as needed.
pub(crate) fn command_line(guidance: &ToolGuidance) -> String {
    std::iter::once(&guidance.program)
        .chain(&guidance.args)
        .map(|word| shell_word(word))
        .collect::<Vec<_>>()
        .join(" ")
}

/// The guidance line for `assessed`'s tool command, if it has one, indented
/// by `indent`. Lumen never runs the command; the line says so.
pub(crate) fn guidance_line(assessed: &Assessed, indent: &str) -> Option<String> {
    tool_action(assessed).map(|guidance| {
        format!(
            "{indent}- To clean it up yourself: {} (Lumen does not run it)",
            command_line(&guidance)
        )
    })
}

/// The AI note as one line: only the closed kind and confidence, never model
/// text.
fn note_line(note: ItemNote, indent: &str) -> String {
    let kind = match note.kind {
        ItemKind::AppSettings => "an app's settings or data",
        ItemKind::UserDocuments => "your documents",
        ItemKind::Cache => "a cache",
        ItemKind::Logs => "logs",
        ItemKind::DownloadsOrInstallers => "downloads or installers",
        ItemKind::DeveloperToolData => "a developer tool's data",
        ItemKind::BackupOrArchive => "a backup or archive",
        ItemKind::Unknown => return format!("{indent}- AI: could not tell what it is"),
    };
    let confidence = match note.confidence {
        JudgmentConfidence::Low => "low",
        JudgmentConfidence::Medium => "medium",
        JudgmentConfidence::High => "high",
    };
    format!("{indent}- AI: likely {kind} ({confidence} confidence)")
}

/// The apps to quit, named, or a generic phrase when the probe named none.
fn apps(free: &FreeWhenClosed) -> String {
    if free.processes.is_empty() {
        "the apps using it".to_owned()
    } else {
        free.processes.join(", ")
    }
}

/// The line for an item kept only because running apps use it, which the
/// policy would propose once they are closed.
fn closing_line(free: &FreeWhenClosed, indent: &str) -> String {
    format!(
        "{indent}- Quit {} and scan again to clean it up",
        apps(free)
    )
}

/// What quitting the apps that keep items in use would let Lumen propose
/// on the next scan, when there is any.
fn closing_summary(assessed: &[Assessed]) -> Option<String> {
    let kept: Vec<&Assessed> = assessed
        .iter()
        .filter(|a| a.free_when_closed.is_some() && a.decision.verdict() == Verdict::Keep)
        .collect();
    if kept.is_empty() {
        return None;
    }
    let bytes: u64 = kept.iter().map(|a| reclaim(a).get()).sum();
    let names: std::collections::BTreeSet<&str> = kept
        .iter()
        .filter_map(|a| a.free_when_closed.as_ref())
        .flat_map(|free| free.processes.iter().map(String::as_str))
        .collect();
    let who = if names.is_empty() {
        "the apps using them".to_owned()
    } else {
        names.into_iter().collect::<Vec<_>>().join(", ")
    };
    Some(format!(
        "{} more could be cleaned up after you quit {who} and scan again ({} kept items in use)",
        human(ByteCount::new(bytes)),
        kept.len()
    ))
}

/// How many review items AI review described, when it described any and
/// left some out (the cap, or an unavailable judge).
fn described_line(assessed: &[Assessed]) -> Option<String> {
    let asked = assessed.iter().filter(|a| a.note_trace.is_some()).count();
    let describable = assessed.iter().filter(|a| describable(a)).count();
    (asked > 0 && describable > asked).then(|| {
        format!(
            "AI review described {asked} of {describable} items left for review (at most {MAX_DESCRIBED}, the largest first)"
        )
    })
}

/// The report as text.
pub(crate) fn to_text(assessed: &[Assessed]) -> String {
    let mut lines = Vec::new();
    for item in assessed {
        lines.push(format!(
            "{:<10} {:>9}  {}",
            verdict_text(item.decision.verdict()),
            human(reclaim(item)),
            item.candidate.path.display()
        ));
        lines.extend(
            item.reasons
                .iter()
                .map(|reason| format!("           - {reason}")),
        );
        lines.extend(guidance_line(item, "           "));
        if item.decision.verdict() == Verdict::Review {
            lines.extend(item.note.map(|note| note_line(note, "           ")));
        }
        if item.decision.verdict() == Verdict::Keep {
            lines.extend(
                item.free_when_closed
                    .as_ref()
                    .map(|free| closing_line(free, "           ")),
            );
        }
    }
    lines.extend(described_line(assessed));
    lines.extend(closing_summary(assessed));
    let proposed: u64 = assessed
        .iter()
        .filter(|a| a.decision.verdict() == Verdict::Quarantine)
        .map(|a| reclaim(a).get())
        .sum();
    lines.push(format!(
        "{} candidates; {} proposed for quarantine (nothing has been moved)",
        assessed.len(),
        human(ByteCount::new(proposed))
    ));
    lines.join("\n") + "\n"
}

/// The report as JSON.
pub(crate) fn to_json(assessed: &[Assessed]) -> Value {
    json!({
        "candidates": assessed.iter().map(|item| json!({
            "path": item.candidate.path.display(),
            "verdict": verdict_text(item.decision.verdict()),
            "reclaimable_bytes": reclaim(item).get(),
            "evidence_hash": item.decision.evidence_hash().to_string(),
            "reasons": item.reasons,
            "tool_command": tool_action(item).map(|guidance| json!({
                "tool": guidance.tool,
                "program": guidance.program,
                "args": guidance.args,
            })),
            "ai_note": item.note.filter(|_| item.decision.verdict() == Verdict::Review).map(|note| json!({
                "kind": note.kind,
                "confidence": note.confidence,
            })),
            "free_when_closed": item.free_when_closed.as_ref().filter(|_| item.decision.verdict() == Verdict::Keep).map(|free| json!({
                "processes": free.processes,
            })),
        })).collect::<Vec<_>>(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn guidance(program: &str, args: &[&str]) -> ToolGuidance {
        ToolGuidance {
            entry: "pnpm-store".into(),
            tool: "Tool".to_owned(),
            program: program.to_owned(),
            args: args.iter().map(|&a| a.to_owned()).collect(),
            removes_data: false,
        }
    }

    #[test]
    fn plain_words_are_joined_as_they_are() {
        assert_eq!(
            command_line(&guidance("xcrun", &["simctl", "delete", "unavailable"])),
            "xcrun simctl delete unavailable"
        );
        assert_eq!(
            command_line(&guidance("npm", &["cache", "clean", "--force"])),
            "npm cache clean --force"
        );
    }

    #[test]
    fn notes_show_only_the_closed_kind_and_confidence() {
        let note = |kind, confidence| ItemNote {
            kind,
            confidence,
            injection_suspected: false,
        };
        assert_eq!(
            note_line(note(ItemKind::Cache, JudgmentConfidence::Low), "  "),
            "  - AI: likely a cache (low confidence)"
        );
        assert_eq!(
            note_line(
                note(ItemKind::DownloadsOrInstallers, JudgmentConfidence::High),
                ""
            ),
            "- AI: likely downloads or installers (high confidence)"
        );
        assert_eq!(
            note_line(note(ItemKind::Unknown, JudgmentConfidence::Medium), ""),
            "- AI: could not tell what it is"
        );
    }

    #[test]
    fn closing_lines_name_the_apps_or_say_the_apps() {
        let named = FreeWhenClosed {
            processes: vec!["Discord".to_owned(), "Spotify".to_owned()],
        };
        assert_eq!(
            closing_line(&named, "  "),
            "  - Quit Discord, Spotify and scan again to clean it up"
        );
        let unnamed = FreeWhenClosed {
            processes: Vec::new(),
        };
        assert_eq!(
            closing_line(&unnamed, ""),
            "- Quit the apps using it and scan again to clean it up"
        );
        assert_eq!(closing_summary(&[]), None);
    }

    #[test]
    fn other_words_are_single_quoted() {
        assert_eq!(
            command_line(&guidance("tool", &["a b", "", "it's", "$HOME", "*"])),
            "tool 'a b' '' 'it'\\''s' '$HOME' '*'"
        );
    }
}
