//! Finding web browser profiles inside apps' `~/Library/Caches` folders.
//!
//! Apps that embed Chromium (CEF, Electron) may put a whole browser profile in
//! their `Caches` folder: `~/Library/Caches/com.spotify.client` holds `Default/`
//! and `Browser/` profiles with `Cookies`, `Login Data` and `Local Storage`.
//! That is the user's sign-in and site data, not a re-creatable cache, so each
//! direct child of `~/Library/Caches` with such an entry below it gets
//! [`Fact::BrowserProfile`] evidence, and the policy keeps it.
//!
//! Only names are compared, exactly (Chromium creates these names as listed),
//! and nothing is opened. A marker counts at most [`MAX_DEPTH`] levels below
//! the child: the profile itself (`Cookies`), a profile folder in a user data
//! folder (`Default/Cookies`), and either a profile's `Network` folder
//! (`Default/Network/Cookies`) or profiles one folder deeper
//! (`cep_cache/<panel>/Cookies`, Adobe CEP). Deeper names are unrelated content,
//! such as a package named `cookies` in a package manager's cache.
//!
//! A scan is read in chunks, so [`WebProfiles::add`] runs over every chunk
//! first and [`WebProfiles::evidence`] over each chunk after. Pure and
//! platform-independent.

use std::collections::HashMap;

use lumen_domain::{
    Basis, Evidence, Fact, FilesystemEntry, Provenance, RawPath, SourceName, Subject, Timestamp,
    UntrustedText,
};

/// Source of [`Fact::BrowserProfile`] evidence.
pub const SOURCE: &str = "macos.web-profile";

/// Entry names that only a browser profile's state has: its cookies, saved
/// sign-ins, autofill data and site storage.
pub const MARKERS: &[&str] = &[
    "Cookies",
    "Login Data",
    "Web Data",
    "Local Storage",
    "Session Storage",
    "IndexedDB",
];

/// How many levels below a `Caches` child a marker may be.
pub const MAX_DEPTH: usize = 3;

/// The `Caches` children that hold a browser profile, by name, each with the
/// first marker found below it (in byte order, so the chunking does not
/// change it).
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct WebProfiles {
    found: HashMap<Vec<u8>, Vec<u8>>,
}

fn caches(home: &RawPath) -> Vec<u8> {
    let mut dir = home.as_bytes().to_vec();
    dir.extend_from_slice(b"/Library/Caches/");
    dir
}

impl WebProfiles {
    /// An empty set.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Pass 1: notes every marker in `chunk` below a child of `home`'s
    /// `Library/Caches`.
    pub fn add(&mut self, home: &RawPath, chunk: &[FilesystemEntry]) {
        let caches = caches(home);
        for entry in chunk {
            let Some(rest) = entry.path.as_bytes().strip_prefix(caches.as_slice()) else {
                continue;
            };
            let Some(slash) = rest.iter().position(|b| *b == b'/') else {
                continue;
            };
            let (child, marker) = (&rest[..slash], &rest[slash + 1..]);
            let parts: Vec<&[u8]> = marker.split(|b| *b == b'/').collect();
            let named = parts
                .last()
                .is_some_and(|last| MARKERS.iter().any(|m| m.as_bytes() == *last));
            if child.is_empty() || parts.len() > MAX_DEPTH || !named {
                continue;
            }
            self.found
                .entry(child.to_vec())
                .and_modify(|first| {
                    if marker < first.as_slice() {
                        marker.clone_into(first);
                    }
                })
                .or_insert_with(|| marker.to_vec());
        }
    }

    /// Pass 2: [`Fact::BrowserProfile`] evidence for every `Caches` child in
    /// `chunk` that holds a profile, in chunk order.
    #[must_use]
    pub fn evidence(
        &self,
        home: &RawPath,
        chunk: &[FilesystemEntry],
        observed_at: Timestamp,
    ) -> Vec<Evidence> {
        let Ok(source) = SourceName::new(SOURCE) else {
            return Vec::new();
        };
        let caches = caches(home);
        chunk
            .iter()
            .filter_map(|entry| {
                let child = entry.path.as_bytes().strip_prefix(caches.as_slice())?;
                let marker = self.found.get(child)?;
                Some(Evidence {
                    subject: Subject::File {
                        identity: entry.identity().clone(),
                    },
                    fact: Fact::BrowserProfile {
                        marker: UntrustedText::new(String::from_utf8_lossy(marker).into_owned()),
                    },
                    basis: Basis::Observed,
                    provenance: Provenance {
                        source: source.clone(),
                        observed_at,
                    },
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;

    use lumen_domain::{
        AccessState, ByteCount, EntryKind, EntryTimes, FileId, FileIdentity, Protection, SizeFacts,
        SizeFlags, VolumeId,
    };

    use super::*;

    fn entry(path: &str, ino: u128) -> FilesystemEntry {
        FilesystemEntry {
            path: RawPath::from_unix_bytes(path.as_bytes().to_vec())
                .unwrap_or_else(|_| unreachable!()),
            kind: EntryKind::Directory,
            size: SizeFacts {
                identity: FileIdentity {
                    volume: VolumeId::new("v").unwrap_or_else(|_| unreachable!()),
                    file: FileId::new(ino),
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

    fn home() -> RawPath {
        RawPath::from_unix_bytes(b"/Users/ana".to_vec()).unwrap_or_else(|_| unreachable!())
    }

    /// The marker of each piece of evidence, as (child inode, marker).
    fn found(chunks: &[&[FilesystemEntry]]) -> Vec<(u128, String)> {
        let mut profiles = WebProfiles::new();
        for chunk in chunks {
            profiles.add(&home(), chunk);
        }
        chunks
            .iter()
            .flat_map(|chunk| profiles.evidence(&home(), chunk, Timestamp::UNIX_EPOCH))
            .filter_map(|e| match (&e.subject, &e.fact) {
                (Subject::File { identity }, Fact::BrowserProfile { marker }) => {
                    Some((identity.file.get(), marker.raw().to_owned()))
                }
                _ => None,
            })
            .collect()
    }

    #[test]
    fn a_cache_with_a_profile_below_it_is_found_across_chunks() {
        let caches = "/Users/ana/Library/Caches";
        let first = [
            entry(&format!("{caches}/com.spotify.client"), 1),
            entry(&format!("{caches}/com.spotify.client/Data"), 2),
        ];
        let second = [
            entry(
                &format!("{caches}/com.spotify.client/Default/Login Data"),
                3,
            ),
            entry(&format!("{caches}/com.spotify.client/Browser/Cookies"), 4),
        ];
        assert_eq!(
            found(&[&first, &second]),
            [(1, "Browser/Cookies".to_owned())],
            "the child itself, citing the first marker in byte order"
        );
    }

    #[test]
    fn markers_count_down_to_three_levels() {
        let caches = "/Users/ana/Library/Caches";
        let chunk = [
            entry(&format!("{caches}/a"), 1),
            entry(&format!("{caches}/a/Cookies"), 11),
            entry(&format!("{caches}/b"), 2),
            entry(&format!("{caches}/b/Default/Network/Cookies"), 12),
            entry(&format!("{caches}/c"), 3),
            entry(&format!("{caches}/c/v6/npm-x/node_modules/IndexedDB"), 13),
            entry(&format!("{caches}/d"), 4),
            entry(&format!("{caches}/d/Default/Cache"), 14),
            entry(&format!("{caches}/d/cookies"), 15),
            entry(&format!("{caches}/e"), 5),
            entry(&format!("{caches}/e/Default/Session Storage"), 16),
        ];
        assert_eq!(
            found(&[&chunk]),
            [
                (1, "Cookies".to_owned()),
                (2, "Default/Network/Cookies".to_owned()),
                (5, "Default/Session Storage".to_owned()),
            ],
            "deeper, other or differently cased names are not markers"
        );
    }

    #[test]
    fn only_direct_children_of_the_users_caches_are_named() {
        let chunk = [
            entry("/Users/ana/Library/Caches/Cookies", 1),
            entry("/Users/ana/Library/Caches/x/Default", 2),
            entry("/Users/ana/Library/Caches/x/Default/Cookies", 3),
            entry("/Users/ana/Library/Application Support/y", 4),
            entry("/Users/ana/Library/Application Support/y/Cookies", 5),
            entry("/Users/bob/Library/Caches/z", 6),
            entry("/Users/bob/Library/Caches/z/Cookies", 7),
        ];
        assert_eq!(found(&[&chunk]), [], "x itself was not in the scan");
    }
}
