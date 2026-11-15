//! The text and JSON forms of a scan summary.

use lumen_app::ScanSummary;
use lumen_app::unread::{Unread, UnreadFolders};
use lumen_domain::{ByteCount, DenialReason, FailureKind, RootCoverage};
use lumen_scan::StopReason;
use serde_json::{Value, json};

/// Decimal units, as Finder and Explorer's "size" use.
pub(crate) fn human(bytes: ByteCount) -> String {
    const UNITS: [&str; 6] = ["B", "kB", "MB", "GB", "TB", "PB"];
    let mut value = bytes.get();
    let mut unit = 0;
    let mut remainder = 0;
    while value >= 1000 && unit + 1 < UNITS.len() {
        remainder = value % 1000;
        value /= 1000;
        unit += 1;
    }
    if unit == 0 {
        format!("{value} B")
    } else {
        format!("{value}.{} {}", remainder / 100, UNITS[unit])
    }
}

fn line(out: &mut String, text: &str) {
    out.push_str(text);
    out.push('\n');
}

fn denials(coverage: &RootCoverage) -> u64 {
    coverage.denied.values().sum()
}

fn failures(coverage: &RootCoverage) -> u64 {
    coverage.failed.values().sum()
}

/// The scan as text for the terminal.
pub(crate) fn to_text(summary: &ScanSummary) -> String {
    let mut out = String::new();
    for r in &summary.roots {
        line(&mut out, &r.root.display());
        line(&mut out, &format!("  files        {}", r.files));
        line(
            &mut out,
            &format!(
                "  size         {} (allocated {})",
                human(r.logical),
                human(r.allocated)
            ),
        );
        let bound = if r.exact {
            ""
        } else {
            " (lower bound: shared blocks not measurable)"
        };
        line(
            &mut out,
            &format!("  reclaimable  {}{bound}", human(r.reclaimable)),
        );
        if r.coverage.is_complete() {
            line(&mut out, "  coverage     complete");
        } else {
            let tcc = r
                .coverage
                .denied
                .get(&DenialReason::Tcc)
                .copied()
                .unwrap_or(0);
            let privacy = if tcc > 0 {
                format!(" ({tcc} by privacy protection)")
            } else {
                String::new()
            };
            line(
                &mut out,
                &format!(
                    "  coverage     INCOMPLETE: {} denied{privacy}, {} failed, {} not scanned; sizes above are lower bounds",
                    denials(&r.coverage),
                    failures(&r.coverage),
                    r.coverage.not_scanned,
                ),
            );
        }
    }
    line(
        &mut out,
        &format!(
            "{} entries, {} directories, {} skipped",
            summary.entries, summary.directories, summary.skipped
        ),
    );
    let partial = match summary.stopped {
        _ if summary.out_of_space => Some(
            "scan stopped: the disk holding the database is nearly full (Lumen keeps 2 GB free); results are partial",
        ),
        Some(StopReason::Cancelled) => Some("scan was cancelled; results are partial"),
        Some(StopReason::Consumer) => Some("scan stopped early; results are partial"),
        None => None,
    };
    if let Some(message) = partial {
        line(&mut out, message);
    }
    if let Some(db) = &summary.database {
        line(
            &mut out,
            &format!("saved scan {} to {}", summary.scan, db.display()),
        );
    }
    line(&mut out, "nothing was modified");
    out
}

/// The scan as JSON.
pub(crate) fn to_json(summary: &ScanSummary) -> Value {
    json!({
        "scan": summary.scan,
        "roots": summary.roots.iter().map(|r| json!({
            "root": r.root.display(),
            "files": r.files,
            "logical": r.logical,
            "allocated": r.allocated,
            "reclaimable_now": r.reclaimable,
            "reclaimable_exact": r.exact,
            "coverage_complete": r.coverage.is_complete(),
            "denied": denials(&r.coverage),
            "failed": failures(&r.coverage),
            "not_scanned": r.coverage.not_scanned,
        })).collect::<Vec<_>>(),
        "entries": summary.entries,
        "directories": summary.directories,
        "skipped": summary.skipped,
        "partial": summary.stopped.is_some(),
        "out_of_space": summary.out_of_space,
        "database": summary.database.as_ref().map(|d| d.display().to_string()),
    })
}

/// Why a folder was not read, in a few words.
fn unread_reason(why: Unread) -> &'static str {
    match why {
        Unread::Denied(DenialReason::Tcc) => {
            "denied by privacy protection (Full Disk Access may help)"
        }
        Unread::Denied(DenialReason::Sip) => "denied by System Integrity Protection",
        Unread::Denied(DenialReason::Posix) => "denied by file permissions",
        Unread::Denied(DenialReason::Acl) => "denied by an access-control list",
        Unread::Denied(DenialReason::Scope) => "outside what this app may read",
        Unread::Denied(_) => "denied",
        Unread::Failed(FailureKind::Vanished) => "removed while scanning",
        Unread::Failed(FailureKind::TimedOut) => "timed out (scanning again may help)",
        Unread::Failed(_) => "read failed",
        Unread::CutShort => "read only in part",
        Unread::NotEntered => "not entered (another disk, excluded or not reached)",
        _ => "not read",
    }
}

/// A stable code for each reason, for JSON: the domain's serde names.
fn unread_code(why: Unread) -> &'static str {
    match why {
        Unread::Denied(DenialReason::Tcc) => "denied.tcc",
        Unread::Denied(DenialReason::Sip) => "denied.sip",
        Unread::Denied(DenialReason::Posix) => "denied.posix",
        Unread::Denied(DenialReason::Acl) => "denied.acl",
        Unread::Denied(DenialReason::Scope) => "denied.scope",
        Unread::Denied(_) => "denied.unknown",
        Unread::Failed(FailureKind::Vanished) => "failed.vanished",
        Unread::Failed(FailureKind::TimedOut) => "failed.timed_out",
        Unread::Failed(_) => "failed.io",
        Unread::CutShort => "cut_short",
        Unread::NotEntered => "not_entered",
        _ => "unknown",
    }
}

/// The folders the scan did not read, as text after the summary.
pub(crate) fn unread_text(unread: &UnreadFolders) -> String {
    let mut out = String::new();
    if unread.total == 0 {
        line(&mut out, "unread folders: none");
        return out;
    }
    line(&mut out, &format!("unread folders ({}):", unread.total));
    for folder in &unread.folders {
        line(
            &mut out,
            &format!(
                "  {}  [{}]",
                folder.path.display(),
                unread_reason(folder.why)
            ),
        );
    }
    let shown = u64::try_from(unread.folders.len()).unwrap_or(u64::MAX);
    if unread.total > shown {
        line(
            &mut out,
            &format!("  ... and {} more", unread.total - shown),
        );
    }
    out
}

/// The folders the scan did not read, as JSON.
pub(crate) fn unread_json(unread: &UnreadFolders) -> Value {
    json!({
        "total": unread.total,
        "folders": unread.folders.iter().map(|f| json!({
            "path": f.path.display(),
            "reason": unread_code(f.why),
        })).collect::<Vec<_>>(),
    })
}

#[cfg(test)]
mod tests {
    use lumen_app::unread::UnreadFolder;
    use lumen_domain::RawPath;

    use super::*;

    fn folders(total: u64, listed: &[(&str, Unread)]) -> UnreadFolders {
        UnreadFolders {
            scan: "0190f0d2-6c1e-7a3b-8c4d-5e6f708192a9"
                .parse()
                .unwrap_or_else(|_| unreachable!()),
            total,
            folders: listed
                .iter()
                .map(|(path, why)| UnreadFolder {
                    path: RawPath::from_unix_bytes(path.as_bytes().to_vec())
                        .unwrap_or_else(|_| unreachable!()),
                    why: *why,
                })
                .collect(),
        }
    }

    #[test]
    fn unread_folders_are_listed_with_why_and_what_is_left_out() {
        let unread = folders(
            3,
            &[
                ("/h/mail", Unread::Denied(DenialReason::Tcc)),
                ("/h/slow", Unread::Failed(FailureKind::TimedOut)),
            ],
        );
        assert_eq!(
            unread_text(&unread),
            "unread folders (3):\n  /h/mail  [denied by privacy protection (Full Disk Access may help)]\n  /h/slow  [timed out (scanning again may help)]\n  ... and 1 more\n"
        );
        assert_eq!(
            unread_json(&unread),
            json!({
                "total": 3,
                "folders": [
                    {"path": "/h/mail", "reason": "denied.tcc"},
                    {"path": "/h/slow", "reason": "failed.timed_out"},
                ],
            })
        );
        assert_eq!(unread_text(&folders(0, &[])), "unread folders: none\n");
    }

    #[test]
    fn human_sizes_use_decimal_units_and_truncate() {
        assert_eq!(human(ByteCount::new(0)), "0 B");
        assert_eq!(human(ByteCount::new(999)), "999 B");
        assert_eq!(human(ByteCount::new(1_500)), "1.5 kB");
        assert_eq!(human(ByteCount::new(86_400_000_000)), "86.4 GB");
        assert_eq!(human(ByteCount::new(u64::MAX)), "18446.7 PB");
    }
}
