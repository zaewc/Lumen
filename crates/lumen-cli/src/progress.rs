//! The scan's live log on standard error: where it started, running totals
//! twice a second, and each directory it could not read or did not enter.
//! Standard output stays free for the summary and `--json`.

use std::time::Duration;

use lumen_app::ScanEvent;
use lumen_domain::{AccessState, DenialReason, FailureKind, RawPath};
use lumen_scan::SkipReason;

use crate::report::human;

/// Running totals at most this often.
const EVERY: Duration = Duration::from_millis(500);
/// Unreadable and skipped directories named one by one; the rest are counted.
const NAMED: u64 = 20;
/// Longest path shown; longer ones lose their middle.
const WIDTH: usize = 90;

/// Turns scan events into log lines.
#[derive(Debug)]
pub(crate) struct ScanLog {
    last_progress: Option<Duration>,
    named: u64,
    unnamed: u64,
}

impl ScanLog {
    pub(crate) const fn new() -> Self {
        Self {
            last_progress: None,
            named: 0,
            unnamed: 0,
        }
    }

    /// The lines for `event`, which happened `at` after the scan started.
    pub(crate) fn lines(&mut self, event: &ScanEvent, at: Duration) -> Vec<String> {
        let stamp = format!("[{:>7.1}s]", at.as_secs_f64());
        match event {
            ScanEvent::Started { roots } => roots
                .iter()
                .map(|root| format!("{stamp} scanning {}", root.display()))
                .collect(),
            ScanEvent::Progress {
                totals,
                allocated,
                current,
            } => {
                if self
                    .last_progress
                    .is_some_and(|last| at.saturating_sub(last) < EVERY)
                {
                    return Vec::new();
                }
                self.last_progress = Some(at);
                let place = current
                    .as_ref()
                    .map(|dir| format!(" · in {}", shorten(dir)))
                    .unwrap_or_default();
                vec![format!(
                    "{stamp} {} entries · {} folders · {} on disk{place}",
                    grouped(totals.entries),
                    grouped(totals.directories),
                    human(*allocated),
                )]
            }
            ScanEvent::Unreadable { path, access } => self.named(
                &stamp,
                &format!("unreadable {} ({})", shorten(path), why(*access)),
            ),
            ScanEvent::Skipped { path, reason } => self.named(
                &stamp,
                &format!("not entered {} ({})", shorten(path), skip(reason)),
            ),
            _ => Vec::new(),
        }
    }

    /// A line naming one directory, until [`NAMED`] have been named.
    fn named(&mut self, stamp: &str, text: &str) -> Vec<String> {
        if self.named < NAMED {
            self.named += 1;
            vec![format!("{stamp} {text}")]
        } else {
            self.unnamed += 1;
            Vec::new()
        }
    }

    /// The closing line, if some directories were counted but not named.
    pub(crate) fn finish(&self) -> Option<String> {
        (self.unnamed > 0).then(|| {
            format!(
                "...and {} more directories not read or not entered; the summary counts them all",
                grouped(self.unnamed)
            )
        })
    }
}

fn why(access: AccessState) -> &'static str {
    match access {
        AccessState::Denied(DenialReason::Tcc) => "privacy protection: grant Full Disk Access",
        AccessState::Denied(DenialReason::Sip) => "system integrity protection",
        AccessState::Denied(_) => "permission denied",
        AccessState::Failed(FailureKind::Vanished) => "removed while scanning",
        AccessState::Failed(FailureKind::TimedOut) => "timed out",
        AccessState::Failed(_) => "read failed",
        AccessState::NotScanned => "not scanned",
        // Never sent as unreadable; kept exhaustive.
        AccessState::Readable => "read",
    }
}

fn skip(reason: &SkipReason) -> &'static str {
    match reason {
        SkipReason::MountPoint => "another volume",
        SkipReason::Alias => "already scanned under another path",
        _ => "could not be opened safely",
    }
}

/// `n` with thousands separators.
fn grouped(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// The path, at most [`WIDTH`] characters, keeping its start and end.
fn shorten(path: &RawPath) -> String {
    let text = path.display();
    let count = text.chars().count();
    if count <= WIDTH {
        return text;
    }
    let head = (WIDTH - 1) / 2;
    let tail = WIDTH - 1 - head;
    let head: String = text.chars().take(head).collect();
    let tail: String = text.chars().skip(count - tail).collect();
    format!("{head}…{tail}")
}

#[cfg(test)]
mod tests {
    use lumen_domain::ByteCount;
    use lumen_scan::ScanProgress;

    use super::*;

    fn path(text: &str) -> RawPath {
        RawPath::from_unix_bytes(text.as_bytes().to_vec()).unwrap_or_else(|e| unreachable!("{e}"))
    }

    fn progress(entries: u64) -> ScanEvent {
        ScanEvent::Progress {
            totals: ScanProgress {
                entries,
                directories: 12,
                skipped: 0,
                file_bytes: ByteCount::new(9_000_000_000),
            },
            allocated: ByteCount::new(4_100_000_000),
            current: Some(path("/Users/t/Library/Caches")),
        }
    }

    #[test]
    fn totals_are_logged_at_most_twice_a_second() {
        let mut log = ScanLog::new();
        let start = log.lines(
            &ScanEvent::Started {
                roots: vec![path("/Users/t")],
            },
            Duration::ZERO,
        );
        assert_eq!(start, ["[    0.0s] scanning /Users/t"]);
        assert_eq!(
            log.lines(&progress(52_311), Duration::from_millis(100)),
            [
                "[    0.1s] 52,311 entries · 12 folders · 4.1 GB on disk · in /Users/t/Library/Caches"
            ]
        );
        assert_eq!(
            log.lines(&progress(60_000), Duration::from_millis(400)),
            Vec::<String>::new()
        );
        assert_eq!(
            log.lines(&progress(70_000), Duration::from_millis(600))
                .len(),
            1
        );
    }

    #[test]
    fn unreadable_directories_are_named_then_counted() {
        let mut log = ScanLog::new();
        let denied = ScanEvent::Unreadable {
            path: path("/Users/t/Desktop"),
            access: AccessState::Denied(DenialReason::Tcc),
        };
        assert_eq!(
            log.lines(&denied, Duration::from_secs(1)),
            ["[    1.0s] unreadable /Users/t/Desktop (privacy protection: grant Full Disk Access)"]
        );
        for _ in 1..NAMED {
            assert_eq!(log.lines(&denied, Duration::from_secs(1)).len(), 1);
        }
        assert!(log.finish().is_none());
        let skipped = ScanEvent::Skipped {
            path: path("/Volumes/Backup"),
            reason: SkipReason::MountPoint,
        };
        assert_eq!(
            log.lines(&skipped, Duration::from_secs(2)),
            Vec::<String>::new()
        );
        assert_eq!(
            log.finish().as_deref(),
            Some("...and 1 more directories not read or not entered; the summary counts them all")
        );
    }

    #[test]
    fn numbers_are_grouped_and_long_paths_keep_both_ends() {
        assert_eq!(grouped(0), "0");
        assert_eq!(grouped(999), "999");
        assert_eq!(grouped(1_000), "1,000");
        assert_eq!(grouped(1_234_567), "1,234,567");
        let long = format!("/start/{}/end", "x".repeat(200));
        let short = shorten(&path(&long));
        assert_eq!(short.chars().count(), WIDTH);
        assert!(short.starts_with("/start/") && short.ends_with("x/end"));
    }
}
