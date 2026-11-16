//! Linking `~/Library` items to an application that is no longer installed
//! (roadmap 16.9, ADR-0068).
//!
//! A removed app's own data stays in `~/Library`, and the item's name is the
//! only clue to who owned it. A name alone is too weak: SDKs, command-line
//! tools and app groups use names shaped like bundle IDs too. So an owner that
//! is not installed is claimed only when:
//!
//! 1. the name is shaped like a bundle ID (three or more labels of
//!    `A–Z a–z 0–9 -`), directly in a location macOS names after the bundle ID
//!    (the same locations and suffixes as [`crate::support`]);
//! 2. macOS made a sandbox container `~/Library/Containers/<id>` for it, or for
//!    an ID that extends it (`<id>.<more>`: an embedded bundle's ID starts with
//!    its app's ID);
//! 3. no installed application is related to it: none has the same ID, an ID
//!    it extends or that extends it, or the same vendor part (the first two
//!    labels).
//!
//! `com.apple.*`, app groups (`group.*`, `systemgroup.*`) and names starting
//! with a ten-character Team ID are never claimed. Whether Spotlight finds the
//! app installed somewhere else is the caller's question (ADR-0059): it passes
//! only the IDs Spotlight did not find to [`link_removed_app_locations`].
//!
//! The links use their own source, [`REMOVED_APP_SOURCE`], so they are never
//! mistaken for links to an installed app. They say nothing about absence:
//! that is established separately, from complete inventories only (ADR-0008).
//! Pure and platform-independent.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use lumen_domain::{
    AppDataKind, AppId, AppIdScheme, Application, EntryKind, FilesystemEntry, RawPath, SourceName,
    Timestamp, UntrustedText,
};

use crate::support::{BY_BUNDLE_ID, Link, child_name, join, link};

/// Source for links to an application that is not installed.
pub const REMOVED_APP_SOURCE: &str = "macos.support-location.removed-app";

/// Fewest dot-separated labels a name needs to count as a bundle ID.
const MIN_LABELS: usize = 3;

/// Length of an Apple Developer Team ID, the prefix of macOS app groups.
const TEAM_ID_LEN: usize = 10;

/// The spelling an ID is reported with: a container's if there is one, else
/// the smallest in byte order, so the order of the scan does not change it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Spelling {
    /// `false` sorts first: a container's spelling wins.
    not_container: bool,
    text: String,
}

/// What pass 1 noted about the conventional `~/Library` locations.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct RemovedApps {
    /// Claimable names, by lowercase form, with the spelling to report.
    names: BTreeMap<String, Spelling>,
    /// Lowercase names of the claimable `~/Library/Containers` children.
    containers: BTreeSet<String>,
}

/// The conventional locations under `home`: path, suffix, kind, and whether
/// it holds sandbox containers.
fn locations(home: &RawPath) -> Vec<(Vec<u8>, &'static str, AppDataKind, bool)> {
    let library = join(home.as_bytes(), "Library");
    BY_BUNDLE_ID
        .iter()
        .map(|(dir, suffix, kind)| {
            (
                join(&library, dir),
                *suffix,
                *kind,
                *kind == AppDataKind::Container,
            )
        })
        .collect()
}

/// The name of `path` without `suffix` (compared without case), if `path` is
/// a direct child of `dir`, valid UTF-8 and shaped like a bundle ID that may
/// be claimed.
fn claimable<'a>(path: &'a [u8], dir: &[u8], suffix: &str) -> Option<&'a str> {
    let name = child_name(path, dir)?;
    let cut = name.len().checked_sub(suffix.len())?;
    // The suffix is ASCII, so a match ends on a character boundary.
    if !name[cut..].eq_ignore_ascii_case(suffix.as_bytes()) {
        return None;
    }
    let id = std::str::from_utf8(&name[..cut]).ok()?;
    (shaped(id) && !excluded(&id.to_ascii_lowercase())).then_some(id)
}

/// Three or more non-empty labels of the characters Apple allows in a bundle
/// ID (`A–Z a–z 0–9 - .`).
fn shaped(id: &str) -> bool {
    let labels: Vec<&str> = id.split('.').collect();
    labels.len() >= MIN_LABELS
        && labels.iter().all(|label| {
            !label.is_empty()
                && label
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        })
}

/// Apple's own names and app groups, which are never claimed.
fn excluded(lower: &str) -> bool {
    let first = lower.split('.').next().unwrap_or_default();
    lower.starts_with("com.apple.")
        || first == "group"
        || first == "systemgroup"
        || (first.len() == TEAM_ID_LEN && first.bytes().all(|b| b.is_ascii_alphanumeric()))
}

/// The first two labels.
fn vendor(lower: &str) -> Option<&str> {
    let mut dots = lower.match_indices('.').map(|(i, _)| i);
    dots.next()?;
    Some(dots.next().map_or(lower, |end| &lower[..end]))
}

impl RemovedApps {
    /// Nothing noted yet.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Pass 1: notes the claimable names in `chunk` directly under `home`'s
    /// conventional locations, and which of them are sandbox containers.
    pub fn add(&mut self, home: &RawPath, chunk: &[FilesystemEntry]) {
        let locations = locations(home);
        for entry in chunk {
            for (dir, suffix, _, container) in &locations {
                let Some(id) = claimable(entry.path.as_bytes(), dir, suffix) else {
                    continue;
                };
                let lower = id.to_ascii_lowercase();
                // Only a folder is a sandbox container; a file of that name
                // corroborates nothing.
                let container = *container && entry.kind == EntryKind::Directory;
                if container {
                    self.containers.insert(lower.clone());
                }
                let spelling = Spelling {
                    not_container: !container,
                    text: id.to_owned(),
                };
                self.names
                    .entry(lower)
                    .and_modify(|kept| {
                        if spelling < *kept {
                            kept.clone_from(&spelling);
                        }
                    })
                    .or_insert(spelling);
            }
        }
    }

    /// The IDs that may belong to a removed app: corroborated by a container
    /// and related to none of the installed `apps`. Spotlight is still to be
    /// asked about each (ADR-0068 rule 1.3).
    #[must_use]
    pub fn candidates(&self, apps: &[Application]) -> BTreeSet<AppId> {
        let installed: Vec<String> = apps
            .iter()
            .flat_map(|app| &app.ids)
            .map(|id| id.value.raw().to_ascii_lowercase())
            .collect();
        let vendors: BTreeSet<&str> = installed.iter().filter_map(|id| vendor(id)).collect();
        self.names
            .iter()
            .filter(|(lower, _)| {
                let extension = format!("{lower}.");
                let corroborated = self.containers.contains(*lower)
                    || self
                        .containers
                        .range(extension.clone()..)
                        .next()
                        .is_some_and(|c| c.starts_with(&extension));
                let related = installed.iter().any(|id| {
                    id == *lower
                        || id.starts_with(&extension)
                        || lower.starts_with(&format!("{id}."))
                }) || vendor(lower).is_some_and(|v| vendors.contains(v));
                corroborated && !related
            })
            .map(|(_, spelling)| AppId {
                scheme: AppIdScheme::BundleId,
                value: UntrustedText::new(spelling.text.clone()),
            })
            .collect()
    }
}

/// Pass 2: links the direct children of the conventional locations under
/// `home` in `entries` to the `claimed` applications they are named after,
/// compared without case. `claimed` comes from [`RemovedApps::candidates`],
/// less the IDs Spotlight found installed.
#[must_use]
pub fn link_removed_app_locations(
    home: &RawPath,
    claimed: &BTreeSet<AppId>,
    entries: &[FilesystemEntry],
    observed_at: Timestamp,
) -> Vec<Link> {
    let Ok(source) = SourceName::new(REMOVED_APP_SOURCE) else {
        return Vec::new();
    };
    if claimed.is_empty() {
        return Vec::new();
    }
    let by_id: HashMap<String, &AppId> = claimed
        .iter()
        .map(|id| (id.value.raw().to_ascii_lowercase(), id))
        .collect();
    let locations = locations(home);
    let mut links = Vec::new();
    for entry in entries {
        for (dir, suffix, kind, _) in &locations {
            let Some(id) = claimable(entry.path.as_bytes(), dir, suffix)
                .and_then(|id| by_id.get(&id.to_ascii_lowercase()))
            else {
                continue;
            };
            links.extend(link(entry, id, *kind, &source, observed_at));
        }
    }
    links
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;

    use lumen_domain::{
        AccessState, ByteCount, EntryTimes, Fact, FileId, FileIdentity, Protection, RelationKind,
        SizeFacts, SizeFlags, Subject, VolumeId,
    };

    use super::*;

    fn home() -> RawPath {
        RawPath::from_unix_bytes(b"/Users/ana".to_vec()).unwrap_or_else(|_| unreachable!())
    }

    fn app(id: &str) -> Application {
        Application {
            ids: BTreeSet::from([AppId {
                scheme: AppIdScheme::BundleId,
                value: UntrustedText::new(id),
            }]),
            name: None,
            version: None,
            vendor: None,
            signature: None,
            install_locations: Vec::new(),
            sources: BTreeSet::new(),
            uninstall: None,
        }
    }

    fn entry(path: &str, ino: u128) -> FilesystemEntry {
        FilesystemEntry {
            path: RawPath::from_unix_bytes(format!("/Users/ana/Library/{path}").into_bytes())
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

    fn entries(paths: &[&str]) -> Vec<FilesystemEntry> {
        (1_u128..).zip(paths).map(|(n, p)| entry(p, n)).collect()
    }

    /// The candidate IDs, as text.
    fn candidates(paths: &[&str], installed: &[&str]) -> Vec<String> {
        let mut noted = RemovedApps::new();
        noted.add(&home(), &entries(paths));
        let apps: Vec<Application> = installed.iter().map(|id| app(id)).collect();
        noted
            .candidates(&apps)
            .into_iter()
            .map(|id| id.value.raw().to_owned())
            .collect()
    }

    #[test]
    fn a_container_corroborates_its_id_in_every_location() {
        let paths = [
            "Containers/net.example.Notes",
            "Caches/net.example.notes",
            "Preferences/net.example.Notes.plist",
            "HTTPStorages/net.example.Notes",
            "Saved Application State/net.example.Notes.savedState",
        ];
        assert_eq!(candidates(&paths, &[]), ["net.example.Notes"]);
        let claimed = BTreeSet::from([AppId {
            scheme: AppIdScheme::BundleId,
            value: UntrustedText::new("net.example.Notes"),
        }]);
        let made =
            link_removed_app_locations(&home(), &claimed, &entries(&paths), Timestamp::UNIX_EPOCH);
        let kinds: Vec<_> = made
            .iter()
            .map(|l| match &l.location.fact {
                Fact::AppDataLocation { kind } => Some(*kind),
                _ => None,
            })
            .collect();
        assert_eq!(
            kinds,
            [
                Some(AppDataKind::Container),
                Some(AppDataKind::Caches),
                Some(AppDataKind::Preferences),
                Some(AppDataKind::WebData),
                Some(AppDataKind::SavedApplicationState),
            ]
        );
        for made in &made {
            assert_eq!(made.evidence.provenance.source.as_str(), REMOVED_APP_SOURCE);
            assert_eq!(
                made.evidence.fact,
                Fact::OwningApplication(claimed.first().cloned().unwrap_or_else(|| unreachable!()))
            );
            assert!(made.relationship.evidence().contains(&made.evidence.id()));
            assert_eq!(made.relationship.kind(), RelationKind::OwnedBy);
            assert_eq!(made.evidence.subject, *made.relationship.from());
            assert!(matches!(
                made.relationship.to(),
                Subject::Application { .. }
            ));
        }
    }

    #[test]
    fn an_extension_container_corroborates_its_app() {
        let found = candidates(
            &[
                "Containers/org.example.Chat.ShareExtension",
                "Preferences/org.example.Chat.plist",
                "Caches/org.example.Chatter",
            ],
            &[],
        );
        assert_eq!(
            found,
            ["org.example.Chat", "org.example.Chat.ShareExtension"]
        );
    }

    #[test]
    fn a_file_named_like_a_container_corroborates_nothing() {
        let mut noted = RemovedApps::new();
        let mut file = entry("Containers/net.example.Tool", 1);
        file.kind = EntryKind::File;
        noted.add(&home(), &[file, entry("Caches/net.example.Tool", 2)]);
        assert!(noted.candidates(&[]).is_empty());
    }

    #[test]
    fn a_name_without_a_container_is_never_claimed() {
        let found = candidates(
            &[
                "Caches/com.segment.storage.abc",
                "Preferences/org.python.python.plist",
                "HTTPStorages/com.vercel.cli",
            ],
            &[],
        );
        assert!(found.is_empty(), "{found:?}");
    }

    #[test]
    fn apple_names_and_app_groups_are_never_claimed() {
        let found = candidates(
            &[
                "Containers/com.apple.Notes",
                "Containers/group.com.example.shared",
                "Containers/systemgroup.com.example.shared",
                "Containers/UBF8T346G9.com.example.oneauth",
                "Application Scripts/group.net.example.app",
            ],
            &[],
        );
        assert!(found.is_empty(), "{found:?}");
    }

    #[test]
    fn anything_related_to_an_installed_app_is_never_claimed() {
        let paths = [
            "Containers/COM.EXAMPLE.WRITER",
            "Containers/com.example.writer.QuickLook",
            "Containers/com.example.errorreporting",
            "Containers/io.vendor.app",
            "Containers/net.other.tool",
        ];
        let found = candidates(&paths, &["com.example.Writer", "io.vendor.app.helper"]);
        assert_eq!(found, ["net.other.tool"]);
    }

    #[test]
    fn only_bundle_id_shaped_direct_children_count() {
        let found = candidates(
            &[
                "Containers/example.notes",
                "Containers/net.example.my app",
                "Containers/net.example..app",
                "Containers/net_example.app.x",
                "Containers/net.example.deep/Data/Library",
                "Preferences/net.example.prefs",
                "Application Support/net.example.support",
            ],
            &[],
        );
        assert!(found.is_empty(), "{found:?}");
        let mut noted = RemovedApps::new();
        let other =
            RawPath::from_unix_bytes(b"/Users/bob".to_vec()).unwrap_or_else(|_| unreachable!());
        noted.add(&other, &entries(&["Containers/net.example.notes"]));
        assert!(noted.candidates(&[]).is_empty(), "another home");
    }

    #[test]
    fn the_reported_spelling_does_not_depend_on_scan_order() {
        let paths = [
            "Caches/net.example.NOTES",
            "Containers/net.example.Notes",
            "Preferences/net.example.notes.plist",
        ];
        let forward = candidates(&paths, &[]);
        let mut reversed = paths;
        reversed.reverse();
        assert_eq!(forward, ["net.example.Notes"], "the container's spelling");
        assert_eq!(candidates(&reversed, &[]), forward);

        let mut chunked = RemovedApps::new();
        let all = entries(&paths);
        chunked.add(&home(), &all[2..]);
        chunked.add(&home(), &all[..2]);
        let mut whole = RemovedApps::new();
        whole.add(&home(), &all);
        assert_eq!(chunked, whole);
    }

    #[test]
    fn only_claimed_ids_are_linked() {
        let made = link_removed_app_locations(
            &home(),
            &BTreeSet::new(),
            &entries(&["Containers/net.example.Notes"]),
            Timestamp::UNIX_EPOCH,
        );
        assert_eq!(made.len(), 0);
        let claimed = BTreeSet::from([AppId {
            scheme: AppIdScheme::BundleId,
            value: UntrustedText::new("net.example.Notes"),
        }]);
        let made = link_removed_app_locations(
            &home(),
            &claimed,
            &entries(&["Containers/net.example.Other", "Caches/com.apple.Notes"]),
            Timestamp::UNIX_EPOCH,
        );
        assert_eq!(made.len(), 0);
    }
}
