//! Web browser profiles anywhere in a scan (security audit FS-3).
//!
//! A browser profile holds the user's sign-ins, cookies and site storage.
//! Chromium and the apps that embed it (Electron, CEF) keep one wherever they
//! like, so a directory with a profile anywhere below it, such as an app's
//! folder in `~/Library/Application Support` or `%LOCALAPPDATA%`, or a
//! backup, takes those sign-ins with it when it is moved. The collector states
//! `Fact::BrowserProfile` for such a directory, and the policy keeps it
//! (`classify.browser-profile`).
//!
//! A profile is recognised by an entry named exactly like one of [`MARKERS`],
//! at any depth: what holds a profile is kept however deep it lies. Only names
//! are compared and nothing is opened. A planted or unrelated entry with such
//! a name only makes Lumen keep more.
//!
//! The scan is read in pages, so [`ProfileHolders::add`] runs over every page
//! first; [`ProfileHolders::marker`] then answers for any directory. Pure and
//! platform-independent.

use std::collections::BTreeMap;
use std::ops::Bound;

use lumen_domain::{EntryKind, FilesystemEntry, UntrustedText};

use crate::cloud::{key, separator};

/// Entry names that only a browser profile's state has: its cookies, saved
/// sign-ins, autofill data and site storage (ADR-0040). Chromium creates them
/// with these exact names on every platform.
pub const MARKERS: &[&str] = &[
    "Cookies",
    "Login Data",
    "Web Data",
    "Local Storage",
    "Session Storage",
    "IndexedDB",
];

/// The directories of one scan that directly contain a profile marker.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ProfileHolders {
    /// The parent of every marker, with a trailing separator, and the first
    /// marker name in it in byte order (so the paging does not change it).
    parents: BTreeMap<Vec<u8>, Vec<u8>>,
}

impl ProfileHolders {
    /// No profiles.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Pass 1: notes the markers in one page of the scan.
    pub fn add(&mut self, page: &[FilesystemEntry]) {
        for entry in page {
            let path = entry.path.as_bytes();
            let sep = separator(&entry.path);
            let Some(end) = path.iter().rposition(|b| *b == sep) else {
                continue;
            };
            let (parent, name) = (&path[..=end], &path[end + 1..]);
            if !MARKERS.iter().any(|m| m.as_bytes() == name) {
                continue;
            }
            self.parents
                .entry(parent.to_vec())
                .and_modify(|first| {
                    if name < first.as_slice() {
                        name.clone_into(first);
                    }
                })
                .or_insert_with(|| name.to_vec());
        }
    }

    /// For a directory with a marker anywhere below it, the first such marker
    /// (in byte order of its parent, then its name), relative to the
    /// directory, e.g. `Default/Network/Cookies`. `None` for anything else.
    #[must_use]
    pub fn marker(&self, entry: &FilesystemEntry) -> Option<UntrustedText> {
        if entry.kind != EntryKind::Directory {
            return None;
        }
        let own = key(&entry.path);
        // Keys with `own` as a prefix sort together, right from `own`.
        let (parent, name) = self
            .parents
            .range::<[u8], _>((Bound::Included(own.as_slice()), Bound::Unbounded))
            .next()
            .filter(|(parent, _)| parent.starts_with(&own))?;
        let mut marker = parent[own.len()..].to_vec();
        marker.extend_from_slice(name);
        Some(UntrustedText::new(
            String::from_utf8_lossy(&marker).into_owned(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;

    use lumen_domain::{
        AccessState, ByteCount, EntryTimes, FileId, FileIdentity, Protection, RawPath, SizeFacts,
        SizeFlags, VolumeId,
    };

    use super::*;

    fn entry(path: RawPath, kind: EntryKind) -> FilesystemEntry {
        FilesystemEntry {
            path,
            kind,
            size: SizeFacts {
                identity: FileIdentity {
                    volume: VolumeId::new("v").unwrap_or_else(|_| unreachable!()),
                    file: FileId::new(1),
                },
                logical: ByteCount::ZERO,
                allocated: ByteCount::ZERO,
                private: None,
                clone_id: None,
                link_count: NonZeroU32::MIN,
                flags: SizeFlags::default(),
            },
            times: EntryTimes::default(),
            protection: Protection::default(),
            access: AccessState::NotScanned,
        }
    }

    fn dir(path: &str) -> FilesystemEntry {
        entry(
            RawPath::from_unix_bytes(path.as_bytes().to_vec()).unwrap_or_else(|_| unreachable!()),
            EntryKind::Directory,
        )
    }

    fn file(path: &str) -> FilesystemEntry {
        FilesystemEntry {
            kind: EntryKind::File,
            ..dir(path)
        }
    }

    fn holders(pages: &[&[FilesystemEntry]]) -> ProfileHolders {
        let mut holders = ProfileHolders::new();
        for page in pages {
            holders.add(page);
        }
        holders
    }

    fn marker(holders: &ProfileHolders, entry: &FilesystemEntry) -> Option<String> {
        holders.marker(entry).map(|m| m.raw().to_owned())
    }

    #[test]
    fn every_directory_above_a_profile_holds_it_at_any_depth() {
        let support = "/Users/ana/Library/Application Support";
        let holders = holders(&[
            &[file(&format!(
                "{support}/Slack/Partitions/x/Network/Cookies"
            ))],
            &[dir(&format!("{support}/Slack/Partitions/x/Local Storage"))],
        ]);
        for (path, found) in [
            (format!("{support}/Slack"), "Partitions/x/Local Storage"),
            (format!("{support}/Slack/Partitions/x"), "Local Storage"),
            (format!("{support}/Slack/Partitions/x/Network"), "Cookies"),
            (support.to_owned(), "Slack/Partitions/x/Local Storage"),
            (
                "/".to_owned(),
                "Users/ana/Library/Application Support/Slack/Partitions/x/Local Storage",
            ),
        ] {
            assert_eq!(
                marker(&holders, &dir(&path)).as_deref(),
                Some(found),
                "{path}"
            );
        }
        for path in [
            format!("{support}/Slac"),
            format!("{support}/Slack-old"),
            format!("{support}/Slack/Partitions/x/Local Storage"),
            format!("{support}/Slack/Cache"),
        ] {
            assert_eq!(marker(&holders, &dir(&path)), None, "{path}");
        }
        assert_eq!(
            marker(&holders, &file(&format!("{support}/Slack"))),
            None,
            "a file holds nothing"
        );
    }

    #[test]
    fn only_exact_marker_names_count() {
        let holders = holders(&[&[
            file("/h/a/cookies"),
            file("/h/b/Cookies.txt"),
            file("/h/c/Login Data-journal"),
            dir("/h/d/Login Data"),
        ]]);
        for path in ["/h/a", "/h/b", "/h/c"] {
            assert_eq!(marker(&holders, &dir(path)), None, "{path}");
        }
        assert_eq!(
            marker(&holders, &dir("/h/d")).as_deref(),
            Some("Login Data")
        );
    }

    #[test]
    fn the_marker_cited_does_not_depend_on_paging() {
        let a = file("/h/p/Web Data");
        let b = file("/h/p/Cookies");
        let one = holders(&[std::slice::from_ref(&a), std::slice::from_ref(&b)]);
        let other = holders(&[&[b], &[a]]);
        assert_eq!(one, other);
        assert_eq!(marker(&one, &dir("/h")).as_deref(), Some("p/Cookies"));
    }

    #[test]
    fn windows_paths_use_backslashes() {
        let windows = |p: &str| {
            let wide: Vec<u16> = p.encode_utf16().collect();
            RawPath::from_windows_wide(&wide).unwrap_or_else(|_| unreachable!())
        };
        let holders = holders(&[&[entry(
            windows(r"C:\Users\ana\AppData\Local\App\User Data\Default\Network\Cookies"),
            EntryKind::File,
        )]]);
        let holder = entry(
            windows(r"C:\Users\ana\AppData\Local\App"),
            EntryKind::Directory,
        );
        let beside = entry(
            windows(r"C:\Users\ana\AppData\Local\Ap"),
            EntryKind::Directory,
        );
        assert_eq!(
            marker(&holders, &holder).as_deref(),
            Some(r"User Data\Default\Network\Cookies")
        );
        assert_eq!(marker(&holders, &beside), None);
    }
}
