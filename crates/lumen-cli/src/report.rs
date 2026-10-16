//! The text and JSON forms of a scan summary.

use lumen_app::ScanSummary;
use lumen_domain::{ByteCount, DenialReason, RootCoverage};
use lumen_scan::StopReason;
use serde_json::{Value, json};

/// Decimal units, as Finder and Explorer's "size" use.
fn human(bytes: ByteCount) -> String {
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
        "database": summary.database.as_ref().map(|d| d.display().to_string()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn human_sizes_use_decimal_units_and_truncate() {
        assert_eq!(human(ByteCount::new(0)), "0 B");
        assert_eq!(human(ByteCount::new(999)), "999 B");
        assert_eq!(human(ByteCount::new(1_500)), "1.5 kB");
        assert_eq!(human(ByteCount::new(86_400_000_000)), "86.4 GB");
        assert_eq!(human(ByteCount::new(u64::MAX)), "18446.7 PB");
    }
}
