//! Linking per-user app data locations to installed applications by macOS naming
//! conventions (roadmap 6.9, ADR-0013).
//!
//! Two strengths of evidence, kept apart by their provenance source so the policy
//! can weigh them:
//!
//! - [`BUNDLE_ID_SOURCE`]: the item is named after an installed app's bundle ID in
//!   a location Apple's frameworks name that way (`~/Library/Caches/<id>`,
//!   `Containers/<id>`, `Preferences/<id>.plist`, ...). Names are compared without
//!   case, as APFS does by default.
//! - [`NAME_SOURCE`]: `~/Library/Application Support/<name>` where exactly one
//!   installed app has that display name. Vendor folders shared by several apps
//!   ("Google", "Microsoft") are never claimed, and short names are ignored.
//!
//! Only direct children of those locations are linked; anything deeper belongs
//! to its parent. Pure and platform-independent.

use std::collections::HashMap;

use lumen_domain::{
    AppId, AppIdScheme, Application, Basis, Evidence, Fact, FilesystemEntry, Provenance, RawPath,
    RelationKind, Relationship, SourceName, Subject, Timestamp,
};

/// Source for links by exact bundle ID.
pub const BUNDLE_ID_SOURCE: &str = "macos.support-location.bundle-id";
/// Source for links by unique application name.
pub const NAME_SOURCE: &str = "macos.support-location.app-name";

/// `~/Library` subdirectories whose children are named `<bundle id><suffix>`.
const BY_BUNDLE_ID: &[(&str, &str)] = &[
    ("Caches", ""),
    ("Containers", ""),
    ("HTTPStorages", ""),
    ("WebKit", ""),
    ("Application Scripts", ""),
    ("Preferences", ".plist"),
    ("Saved Application State", ".savedState"),
    ("Cookies", ".binarycookies"),
];

/// Shortest application name that may claim an `Application Support` folder.
const MIN_NAME_LEN: usize = 3;

/// One ownership link: the evidence and the relationship that cites it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    /// `OwningApplication` evidence about the location.
    pub evidence: Evidence,
    /// `location owned_by application`, citing the evidence.
    pub relationship: Relationship,
}

/// The last path component and the parent directory, if `path` is a direct
/// child of `dir` (a directory path without trailing slash).
fn child_name<'a>(path: &'a [u8], dir: &[u8]) -> Option<&'a [u8]> {
    let rest = path.strip_prefix(dir)?.strip_prefix(b"/")?;
    (!rest.is_empty() && !rest.contains(&b'/')).then_some(rest)
}

fn join(base: &[u8], part: &str) -> Vec<u8> {
    let mut out = base.to_vec();
    out.push(b'/');
    out.extend_from_slice(part.as_bytes());
    out
}

fn link(
    entry: &FilesystemEntry,
    app: &AppId,
    source: &SourceName,
    observed_at: Timestamp,
) -> Option<Link> {
    let subject = Subject::File {
        identity: entry.identity().clone(),
    };
    let evidence = Evidence {
        subject: subject.clone(),
        fact: Fact::OwningApplication(app.clone()),
        basis: Basis::Observed,
        provenance: Provenance {
            source: source.clone(),
            observed_at,
        },
    };
    let relationship = Relationship::new(
        subject,
        RelationKind::OwnedBy,
        Subject::Application { id: app.clone() },
        [evidence.id()],
    )
    .ok()?;
    Some(Link {
        evidence,
        relationship,
    })
}

/// Links the direct children of the conventional locations under `home` to
/// the installed `apps` they are named after.
pub fn link_support_locations(
    home: &RawPath,
    apps: &[Application],
    entries: &[FilesystemEntry],
    observed_at: Timestamp,
) -> Vec<Link> {
    let (Ok(by_id_source), Ok(by_name_source)) = (
        SourceName::new(BUNDLE_ID_SOURCE),
        SourceName::new(NAME_SOURCE),
    ) else {
        return Vec::new();
    };
    let mut by_id: HashMap<String, &AppId> = HashMap::new();
    let mut by_name: HashMap<String, Vec<&AppId>> = HashMap::new();
    for app in apps {
        for id in app
            .ids
            .iter()
            .filter(|id| id.scheme == AppIdScheme::BundleId)
        {
            by_id.insert(id.value.raw().to_lowercase(), id);
            if let Some(name) = app.name.as_ref().map(|n| n.raw().to_lowercase()) {
                by_name.entry(name).or_default().push(id);
            }
        }
    }
    let library = join(home.as_bytes(), "Library");
    let support = join(&library, "Application Support");

    let mut links = Vec::new();
    for entry in entries {
        let path = entry.path.as_bytes();
        for (dir, suffix) in BY_BUNDLE_ID {
            let Some(name) = child_name(path, &join(&library, dir)) else {
                continue;
            };
            let Some(id) = std::str::from_utf8(name)
                .ok()
                .and_then(|n| {
                    n.to_lowercase()
                        .strip_suffix(&suffix.to_lowercase())
                        .map(str::to_owned)
                })
                .and_then(|n| by_id.get(&n))
            else {
                continue;
            };
            links.extend(link(entry, id, &by_id_source, observed_at));
        }
        let unique = child_name(path, &support)
            .and_then(|name| std::str::from_utf8(name).ok())
            .map(str::to_lowercase)
            .filter(|name| name.chars().count() >= MIN_NAME_LEN)
            .and_then(|name| by_name.get(&name))
            .and_then(|ids| {
                let first = ids.first()?;
                ids.iter().all(|id| id == first).then_some(*first)
            });
        if let Some(id) = unique {
            links.extend(link(entry, id, &by_name_source, observed_at));
        }
    }
    links
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::num::NonZeroU32;

    use lumen_domain::{
        AccessState, ByteCount, EntryKind, EntryTimes, FileId, FileIdentity, Protection, SizeFacts,
        SizeFlags, UntrustedText, VolumeId,
    };

    use super::*;

    fn app(id: &str, name: &str) -> Application {
        Application {
            ids: BTreeSet::from([AppId {
                scheme: AppIdScheme::BundleId,
                value: UntrustedText::new(id),
            }]),
            name: Some(UntrustedText::new(name)),
            version: None,
            vendor: None,
            signature: None,
            install_locations: Vec::new(),
            sources: BTreeSet::new(),
        }
    }

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

    /// (bundle ID, provenance source) of every link, sorted.
    fn links(entries: &[FilesystemEntry]) -> Vec<(String, String)> {
        let home =
            RawPath::from_unix_bytes(b"/Users/ana".to_vec()).unwrap_or_else(|_| unreachable!());
        let apps = [
            app("com.tinyspeck.slackmacgap", "Slack"),
            app("com.google.Chrome", "Google Chrome"),
            app("com.google.drivefs", "Google Drive"),
            app("com.example.ab", "AB"),
        ];
        let mut out: Vec<_> = link_support_locations(&home, &apps, entries, Timestamp::UNIX_EPOCH)
            .into_iter()
            .filter_map(|l| match l.relationship.to() {
                Subject::Application { id } => Some((
                    id.value.raw().to_owned(),
                    l.evidence.provenance.source.as_str().to_owned(),
                )),
                _ => None,
            })
            .collect();
        out.sort();
        out
    }

    #[test]
    fn bundle_id_locations_link_with_strong_provenance() {
        let found = links(&[
            entry("/Users/ana/Library/Caches/com.tinyspeck.slackmacgap", 1),
            entry("/Users/ana/Library/Containers/COM.GOOGLE.CHROME", 2),
            entry("/Users/ana/Library/Preferences/com.google.drivefs.plist", 3),
            entry(
                "/Users/ana/Library/Saved Application State/com.google.Chrome.savedState",
                4,
            ),
        ]);
        let ids: Vec<_> = found
            .iter()
            .map(|(id, source)| (id.as_str(), source.as_str()))
            .collect();
        assert_eq!(ids.len(), 4);
        assert!(ids.iter().all(|(_, source)| *source == BUNDLE_ID_SOURCE));
        assert!(
            ids.contains(&("com.google.Chrome", BUNDLE_ID_SOURCE)),
            "case-insensitive"
        );
        assert!(
            ids.contains(&("com.google.drivefs", BUNDLE_ID_SOURCE)),
            "suffix stripped"
        );
    }

    #[test]
    fn unique_names_link_weakly_and_shared_or_short_names_never() {
        let found = links(&[
            entry("/Users/ana/Library/Application Support/Slack", 1),
            entry("/Users/ana/Library/Application Support/Google", 2),
            entry("/Users/ana/Library/Application Support/AB", 3),
            entry("/Users/ana/Library/Application Support/Unknown App", 4),
        ]);
        assert_eq!(
            found
                .iter()
                .map(|(id, source)| (id.as_str(), source.as_str()))
                .collect::<Vec<_>>(),
            [("com.tinyspeck.slackmacgap", NAME_SOURCE)]
        );
    }

    #[test]
    fn only_direct_children_of_the_conventional_locations_link() {
        let found = links(&[
            entry(
                "/Users/ana/Library/Caches/com.tinyspeck.slackmacgap/Cache.db",
                1,
            ),
            entry("/Users/ana/Library/Caches", 2),
            entry("/Users/ana/Downloads/com.tinyspeck.slackmacgap", 3),
            entry("/Users/bob/Library/Caches/com.tinyspeck.slackmacgap", 4),
            entry(
                "/Users/ana/Library/Caches/com.tinyspeck.slackmacgap.plist",
                5,
            ),
        ]);
        assert_eq!(found.len(), 0, "{found:?}");
    }

    #[test]
    fn links_cite_their_evidence() {
        let home =
            RawPath::from_unix_bytes(b"/Users/ana".to_vec()).unwrap_or_else(|_| unreachable!());
        let made = link_support_locations(
            &home,
            &[app("com.tinyspeck.slackmacgap", "Slack")],
            &[entry(
                "/Users/ana/Library/Caches/com.tinyspeck.slackmacgap",
                1,
            )],
            Timestamp::UNIX_EPOCH,
        );
        assert_eq!(made.len(), 1);
        let first = &made[0];
        assert!(first.relationship.evidence().contains(&first.evidence.id()));
        assert_eq!(first.relationship.kind(), RelationKind::OwnedBy);
        assert_eq!(first.evidence.subject, *first.relationship.from());
    }
}
