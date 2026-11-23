//! Items in the user's log and temporary folders (ADR-0070, roadmap 15.11).
//!
//! Each file or folder directly inside a scratch root (macOS
//! `~/Library/Logs`, the Windows user's `%LOCALAPPDATA%\Temp`) becomes a
//! candidate of its own with:
//!
//! - [`Fact::ScratchLocation`], always;
//! - [`Fact::NewestChange`], the newest modification, creation or
//!   metadata-change time of the item and every entry below it, dated by the
//!   scan. Only when the item and its whole subtree were read and every entry
//!   reported both a modification and a metadata-change time; otherwise
//!   nothing, so the policy keeps the item as of unknown age (ADR-0008);
//! - [`Fact::DiagnosticReport`], when the item is or holds a crash, spin or
//!   diagnostic report, recognised by entry names alone ([`is_report`]).
//!
//! The root itself is never a candidate. Links and special files are not
//! items, and the scanner does not follow them. Only stored metadata is read;
//! no file is opened. The policy reviews old items and keeps the rest; no
//! rule proposes them for quarantine.

use std::collections::{BTreeMap, HashSet};

use lumen_domain::{
    Basis, EntryKind, Evidence, Fact, FilesystemEntry, PathFlavor, Provenance, RawPath,
    ScratchKind, SourceName, Subject, Timestamp, UntrustedText,
};

use crate::cloud::{key, separator};

/// The source named in the provenance of the facts recorded here.
pub const SCRATCH_SOURCE: &str = "lumen.scratch";

/// Extensions of crash, spin and diagnostic reports: those Apple's Console
/// guide names (`.ips`, `.spin`, `.diag`, `.dpsub`) and `.crash`, compared
/// without case.
pub const REPORT_EXTENSIONS: &[&str] = &["ips", "crash", "spin", "diag", "dpsub"];

/// The folder macOS keeps a user's crash and diagnostic reports in.
pub const REPORT_FOLDER: &str = "DiagnosticReports";

/// Whether an entry called `name` is a report, or the folder of reports.
#[must_use]
pub fn is_report(name: &[u8]) -> bool {
    if name.eq_ignore_ascii_case(REPORT_FOLDER.as_bytes()) {
        return true;
    }
    name.iter().rposition(|&b| b == b'.').is_some_and(|dot| {
        let extension = name.get(dot + 1..).unwrap_or_default();
        REPORT_EXTENSIONS
            .iter()
            .any(|known| known.as_bytes().eq_ignore_ascii_case(extension))
    })
}

/// The user's scratch roots on `flavor`'s platform: `~/Library/Logs` below
/// `home` on macOS, `Temp` below `local_app_data` on Windows. Nothing
/// elsewhere, and nothing when the folder they are below is unknown.
#[must_use]
pub fn roots(
    macos_home: Option<&RawPath>,
    windows_local_app_data: Option<&RawPath>,
) -> Vec<(RawPath, ScratchKind)> {
    let below = |base: &RawPath, names: &[&str]| {
        let sep = separator(base);
        let mut bytes = base.as_bytes().to_vec();
        for name in names {
            if bytes.last() != Some(&sep) {
                bytes.push(sep);
            }
            bytes.extend_from_slice(name.as_bytes());
        }
        RawPath::from_stored(base.flavor(), bytes).ok()
    };
    let logs = macos_home
        .filter(|home| home.flavor() == PathFlavor::Unix)
        .and_then(|home| below(home, &["Library", "Logs"]))
        .map(|root| (root, ScratchKind::Logs));
    let temporary = windows_local_app_data
        .filter(|base| base.flavor() == PathFlavor::Windows)
        .and_then(|base| below(base, &["Temp"]))
        .map(|root| (root, ScratchKind::Temporary));
    logs.into_iter().chain(temporary).collect()
}

/// What the scan saw of one item so far.
#[derive(Debug, Clone)]
struct Item {
    kind: ScratchKind,
    /// The item's own entry, once its listing has been seen.
    entry: Option<FilesystemEntry>,
    /// The newest change time seen in it.
    newest: Option<Timestamp>,
    /// An entry in it lacked a time, its metadata or its listing.
    unknown: bool,
    /// The first report in it, relative to the item.
    report: Option<Vec<u8>>,
}

/// The items of the scratch roots, gathered from a scan read a page at a
/// time, in any order.
#[derive(Debug, Clone)]
pub struct ScratchItems {
    /// Each root with a trailing separator, and its kind.
    roots: Vec<(Vec<u8>, ScratchKind)>,
    /// Directories whose listing was not read in full.
    unlisted: HashSet<Vec<u8>>,
    /// Items by path.
    items: BTreeMap<Vec<u8>, Item>,
}

impl ScratchItems {
    /// Gathers the items of `roots`; `unlisted` are the directories the scan
    /// did not list in full.
    pub fn new(
        roots: &[(RawPath, ScratchKind)],
        unlisted: impl IntoIterator<Item = RawPath>,
    ) -> Self {
        Self {
            roots: roots
                .iter()
                .map(|(root, kind)| (key(root), *kind))
                .collect(),
            unlisted: unlisted.into_iter().map(|path| key(&path)).collect(),
            items: BTreeMap::new(),
        }
    }

    /// Notes one page of the scan.
    pub fn observe(&mut self, page: &[FilesystemEntry]) {
        if self.roots.is_empty() {
            return;
        }
        for entry in page {
            self.note(entry);
        }
    }

    fn note(&mut self, entry: &FilesystemEntry) {
        let path = entry.path.as_bytes();
        let sep = separator(&entry.path);
        let Some((root, kind)) = self
            .roots
            .iter()
            .find(|(root, _)| path.len() > root.len() && path.starts_with(root))
        else {
            return;
        };
        let (root_len, kind) = (root.len(), *kind);
        let rest = &path[root_len..];
        let name_len = rest.iter().position(|&b| b == sep).unwrap_or(rest.len());
        let item_path = path[..root_len + name_len].to_vec();
        let own = name_len == rest.len();
        if own && !matches!(entry.kind, EntryKind::File | EntryKind::Directory) {
            return;
        }
        let item = self.items.entry(item_path).or_insert(Item {
            kind,
            entry: None,
            newest: None,
            unknown: false,
            report: None,
        });
        if own {
            item.entry = Some(entry.clone());
        }
        let times = entry.times;
        let read = entry.kind == EntryKind::Directory || entry.access.is_observed();
        let listed =
            entry.kind != EntryKind::Directory || !self.unlisted.contains(&key(&entry.path));
        if !read || !listed || times.modified.is_none() || times.changed.is_none() {
            item.unknown = true;
        }
        let newest = [times.modified, times.changed, times.created]
            .into_iter()
            .flatten()
            .max();
        item.newest = item.newest.max(newest);
        let name_start = path.iter().rposition(|&b| b == sep).map_or(0, |at| at + 1);
        if is_report(&path[name_start..]) {
            // Relative to the item, or the item's own name.
            let relative = if own {
                rest.to_vec()
            } else {
                rest[name_len + 1..].to_vec()
            };
            if item.report.as_ref().is_none_or(|first| relative < *first) {
                item.report = Some(relative);
            }
        }
    }

    /// The items whose own entry the scan saw, each with its facts dated by
    /// the scan (`scanned_at`), in path order.
    #[must_use]
    pub fn finish(self, scanned_at: Timestamp) -> Vec<(FilesystemEntry, Vec<Evidence>)> {
        let Ok(source) = SourceName::new(SCRATCH_SOURCE) else {
            return Vec::new();
        };
        self.items
            .into_values()
            .filter_map(|item| {
                let entry = item.entry?;
                let subject = Subject::File {
                    identity: entry.identity().clone(),
                };
                let mut facts = vec![Fact::ScratchLocation { kind: item.kind }];
                if !item.unknown {
                    facts.extend(item.newest.map(|at| Fact::NewestChange {
                        at,
                        as_of: scanned_at,
                    }));
                }
                facts.extend(item.report.map(|marker| Fact::DiagnosticReport {
                    marker: UntrustedText::new(String::from_utf8_lossy(&marker).into_owned()),
                }));
                let evidence = facts
                    .into_iter()
                    .map(|fact| Evidence {
                        subject: subject.clone(),
                        fact,
                        basis: Basis::Observed,
                        provenance: Provenance {
                            source: source.clone(),
                            observed_at: scanned_at,
                        },
                    })
                    .collect();
                Some((entry, evidence))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;

    use lumen_domain::{
        AccessState, ByteCount, EntryTimes, FileId, FileIdentity, Protection, SizeFacts, SizeFlags,
        VolumeId,
    };

    use super::*;

    const LOGS: &str = "/Users/ana/Library/Logs";

    fn at(secs: i64) -> Timestamp {
        Timestamp::from_unix(secs, 0).unwrap_or_else(|_| unreachable!())
    }

    fn unix(path: &str) -> RawPath {
        RawPath::from_unix_bytes(path.as_bytes().to_vec()).unwrap_or_else(|_| unreachable!())
    }

    fn windows(path: &str) -> RawPath {
        let wide: Vec<u16> = path.encode_utf16().collect();
        RawPath::from_windows_wide(&wide).unwrap_or_else(|_| unreachable!())
    }

    /// A readable entry with modification and change time `secs`.
    fn entry(path: RawPath, kind: EntryKind, n: u128, secs: i64) -> FilesystemEntry {
        FilesystemEntry {
            path,
            kind,
            size: SizeFacts {
                identity: FileIdentity {
                    volume: VolumeId::new("v").unwrap_or_else(|_| unreachable!()),
                    file: FileId::new(n),
                },
                logical: ByteCount::ZERO,
                allocated: ByteCount::ZERO,
                private: None,
                clone_id: None,
                link_count: NonZeroU32::MIN,
                flags: SizeFlags::default(),
            },
            times: EntryTimes {
                modified: Some(at(secs)),
                accessed: None,
                changed: Some(at(secs)),
                created: None,
            },
            protection: Protection::default(),
            access: AccessState::Readable,
        }
    }

    fn dir(path: &str, n: u128, secs: i64) -> FilesystemEntry {
        entry(unix(path), EntryKind::Directory, n, secs)
    }

    fn file(path: &str, n: u128, secs: i64) -> FilesystemEntry {
        entry(unix(path), EntryKind::File, n, secs)
    }

    fn gather(
        pages: &[&[FilesystemEntry]],
        unlisted: &[&str],
    ) -> Vec<(FilesystemEntry, Vec<Evidence>)> {
        let mut items = ScratchItems::new(
            &roots(Some(&unix("/Users/ana")), None),
            unlisted.iter().map(|path| unix(path)),
        );
        for page in pages {
            items.observe(page);
        }
        items.finish(at(1_000))
    }

    /// The facts of the item at `path`.
    fn facts(found: &[(FilesystemEntry, Vec<Evidence>)], path: &str) -> Vec<Fact> {
        found
            .iter()
            .find(|(entry, _)| entry.path.as_bytes() == path.as_bytes())
            .map(|(_, evidence)| evidence.iter().map(|e| e.fact.clone()).collect())
            .unwrap_or_default()
    }

    fn logs() -> Fact {
        Fact::ScratchLocation {
            kind: ScratchKind::Logs,
        }
    }

    fn newest(secs: i64) -> Fact {
        Fact::NewestChange {
            at: at(secs),
            as_of: at(1_000),
        }
    }

    #[test]
    fn roots_are_the_users_logs_on_macos_and_temp_on_windows() {
        let found = roots(
            Some(&unix("/Users/ana")),
            Some(&windows(r"C:\Users\ana\AppData\Local")),
        );
        assert_eq!(
            found,
            [
                (unix(LOGS), ScratchKind::Logs),
                (
                    windows(r"C:\Users\ana\AppData\Local\Temp"),
                    ScratchKind::Temporary
                ),
            ]
        );
        // A path of the other platform's form is never a root.
        assert_eq!(
            roots(Some(&windows(r"C:\Users\ana")), Some(&unix("/home/ana"))),
            []
        );
        assert_eq!(roots(None, None), []);
    }

    #[test]
    fn each_direct_child_is_an_item_dated_by_its_newest_change() {
        let found = gather(
            &[
                &[
                    dir(LOGS, 1, 900),
                    dir(&format!("{LOGS}/App"), 2, 100),
                    file(&format!("{LOGS}/old.log"), 3, 50),
                ],
                &[
                    dir(&format!("{LOGS}/App/sub"), 4, 200),
                    file(&format!("{LOGS}/App/sub/a.log"), 5, 300),
                    file("/Users/ana/Library/Caches/x.log", 6, 10),
                ],
            ],
            &[],
        );
        let paths: Vec<&[u8]> = found.iter().map(|(e, _)| e.path.as_bytes()).collect();
        assert_eq!(
            paths,
            [
                format!("{LOGS}/App").as_bytes(),
                format!("{LOGS}/old.log").as_bytes()
            ],
            "the root, nested entries and other folders are not items"
        );
        assert_eq!(facts(&found, &format!("{LOGS}/App")), [logs(), newest(300)]);
        assert_eq!(
            facts(&found, &format!("{LOGS}/old.log")),
            [logs(), newest(50)]
        );
    }

    #[test]
    fn creation_time_counts_as_a_change() {
        let mut copied = file(&format!("{LOGS}/copied.log"), 2, 10);
        copied.times.created = Some(at(700));
        let found = gather(&[&[copied]], &[]);
        assert_eq!(
            facts(&found, &format!("{LOGS}/copied.log")),
            [logs(), newest(700)]
        );
    }

    #[test]
    fn age_is_unknown_when_anything_below_was_not_read_or_dated() {
        let item = format!("{LOGS}/App");
        let mut no_change_time = file(&format!("{item}/a.log"), 3, 10);
        no_change_time.times.changed = None;
        let mut unread = file(&format!("{item}/b.log"), 4, 10);
        unread.access = AccessState::NotScanned;
        for (below, unlisted) in [
            (no_change_time, None),
            (unread, None),
            (
                dir(&format!("{item}/sub"), 5, 10),
                Some(format!("{item}/sub")),
            ),
        ] {
            let unlisted: Vec<&str> = unlisted.iter().map(String::as_str).collect();
            let found = gather(&[&[dir(&item, 2, 10), below]], &unlisted);
            assert_eq!(facts(&found, &item), [logs()]);
        }
    }

    #[test]
    fn reports_are_recognised_by_name_at_any_depth() {
        let found = gather(
            &[&[
                file(&format!("{LOGS}/Safari.IPS"), 2, 10),
                dir(&format!("{LOGS}/App"), 3, 10),
                file(&format!("{LOGS}/App/b/z.crash"), 4, 10),
                file(&format!("{LOGS}/App/a.spin"), 5, 10),
                dir(&format!("{LOGS}/DiagnosticReports"), 6, 10),
                file(&format!("{LOGS}/plain.log"), 7, 10),
            ]],
            &[],
        );
        let marker = |path: &str| {
            facts(&found, &format!("{LOGS}/{path}"))
                .into_iter()
                .find_map(|fact| match fact {
                    Fact::DiagnosticReport { marker } => Some(marker.raw().to_owned()),
                    _ => None,
                })
        };
        assert_eq!(marker("Safari.IPS").as_deref(), Some("Safari.IPS"));
        assert_eq!(
            marker("App").as_deref(),
            Some("a.spin"),
            "the first by path"
        );
        assert_eq!(
            marker("DiagnosticReports").as_deref(),
            Some("DiagnosticReports")
        );
        assert_eq!(marker("plain.log"), None);
        assert!(!is_report(b"ips"), "a name without an extension");
        assert!(!is_report(b"report.ipsx"));
    }

    #[test]
    fn links_are_not_items() {
        let link = entry(unix(&format!("{LOGS}/link")), EntryKind::Symlink, 2, 10);
        assert_eq!(gather(&[&[link]], &[]).len(), 0);
    }
}
