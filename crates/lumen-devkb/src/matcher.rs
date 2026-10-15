//! Matching knowledge-base entries against scanned entries (roadmap 6.16).
//!
//! Each [`Location`] is resolved against an [`Environment`] (the machine's home,
//! known folders and environment variables, as the platform adapter read them)
//! and compared component by component with scanned paths. A match is
//! `KnowledgeBaseMatch` evidence about that object; it says what the object is,
//! never what to do with it.
//!
//! Rules:
//!
//! - only the location itself matches, never its descendants (they belong to it
//!   through `contains` edges);
//! - only regular files and directories match: links and reparse points are other
//!   objects (ADR-0017);
//! - names compare without case on macOS and Windows, whose default volumes are
//!   case-insensitive, and exactly elsewhere;
//! - a project location matches only if one of its markers was itself scanned:
//!   an unscanned marker is unknown, so no evidence is produced.

use std::collections::{BTreeSet, HashSet};

use lumen_domain::{
    Basis, EntryKind, Evidence, Fact, FilesystemEntry, PathFlavor, Platform, Provenance, RawPath,
    SourceName, Subject, Timestamp,
};

use crate::{Entry, KnowledgeBase, Location, Root};

/// Provenance source of match evidence.
pub const SOURCE: &str = "devkb";

/// Where the roots of entry locations are on this machine. Paths must be
/// absolute; a root that is `None` (or a variable that is unset) makes its
/// locations inapplicable, never "absent".
#[derive(Debug, Clone)]
pub struct Environment {
    /// The running platform.
    pub platform: Platform,
    /// The user's home directory.
    pub home: Option<RawPath>,
    /// Windows `FOLDERID_LocalAppData`.
    pub local_app_data: Option<RawPath>,
    /// Windows `FOLDERID_RoamingAppData`.
    pub roaming_app_data: Option<RawPath>,
    /// Environment variables that name directories, by exact name.
    pub vars: Vec<(String, RawPath)>,
}

/// One path component, folded for comparison.
type Component = String;

fn case_insensitive(platform: Platform) -> bool {
    matches!(
        platform,
        Platform::Macos | Platform::Windows | Platform::Ios
    )
}

/// The non-empty components of a path, folded for `platform`.
fn components(path: &RawPath, platform: Platform) -> Vec<Component> {
    let separators: &[u8] = match path.flavor() {
        PathFlavor::Unix => b"/",
        PathFlavor::Windows => b"\\/",
    };
    let fold = case_insensitive(platform);
    path.as_bytes()
        .split(|b| separators.contains(b))
        .filter(|part| !part.is_empty())
        .map(|part| {
            let text = String::from_utf8_lossy(part);
            if fold {
                text.to_lowercase()
            } else {
                text.into_owned()
            }
        })
        .collect()
}

/// A location's pattern relative to its root, folded for `platform`; `*` stays
/// a wildcard.
fn pattern(path: &str, platform: Platform) -> Vec<Component> {
    let fold = case_insensitive(platform);
    path.split('/')
        .filter(|part| !part.is_empty())
        .map(|part| {
            if fold {
                part.to_lowercase()
            } else {
                part.to_owned()
            }
        })
        .collect()
}

fn fits(names: &[Component], pattern: &[Component]) -> bool {
    names.len() == pattern.len() && names.iter().zip(pattern).all(|(n, p)| p == "*" || n == p)
}

/// A location resolved against the environment.
struct Resolved<'kb> {
    entry: &'kb Entry,
    /// The absolute root, or `None` for a project location.
    root: Option<Vec<Component>>,
    pattern: Vec<Component>,
    markers: Vec<Vec<Component>>,
}

fn resolve<'kb>(
    entry: &'kb Entry,
    location: &Location,
    env: &Environment,
) -> Option<Resolved<'kb>> {
    if !location.platforms.contains(&env.platform) {
        return None;
    }
    let root = match &location.root {
        Root::Home => Some(env.home.as_ref()?),
        Root::LocalAppData => Some(env.local_app_data.as_ref()?),
        Root::RoamingAppData => Some(env.roaming_app_data.as_ref()?),
        Root::EnvVar { name } => Some(
            env.vars
                .iter()
                .find(|(var, _)| var == name)
                .map(|(_, path)| path)?,
        ),
        Root::Project => None,
    };
    Some(Resolved {
        entry,
        root: root.map(|r| components(r, env.platform)),
        pattern: pattern(&location.path, env.platform),
        markers: location
            .markers
            .iter()
            .map(|m| pattern(m, env.platform))
            .collect(),
    })
}

impl Resolved<'_> {
    fn matches(&self, names: &[Component], scanned: &HashSet<Vec<Component>>) -> bool {
        if let Some(root) = &self.root {
            return names.len() == root.len() + self.pattern.len()
                && names.starts_with(root)
                && fits(&names[root.len()..], &self.pattern);
        }
        // A project root is whatever precedes the pattern; it must be a real
        // directory, never the filesystem root.
        let Some(split) = names.len().checked_sub(self.pattern.len()) else {
            return false;
        };
        split > 0
            && fits(&names[split..], &self.pattern)
            && self.markers.iter().any(|marker| {
                let mut path = names[..split].to_vec();
                path.extend_from_slice(marker);
                scanned.contains(&path)
            })
    }
}

impl KnowledgeBase {
    /// `KnowledgeBaseMatch` evidence for every scanned file or directory that is
    /// one of this knowledge base's locations in `env`, at most once per object
    /// and entry.
    pub fn matches(
        &self,
        env: &Environment,
        scanned: &[FilesystemEntry],
        observed_at: Timestamp,
    ) -> Vec<Evidence> {
        let Ok(source) = SourceName::new(SOURCE) else {
            return Vec::new();
        };
        let resolved: Vec<Resolved<'_>> = self
            .entries()
            .iter()
            .flat_map(|entry| {
                entry
                    .locations
                    .iter()
                    .filter_map(|l| resolve(entry, l, env))
            })
            .collect();
        // Markers are looked up among scanned paths; keep only paths that could
        // be one, rather than every path of the scan.
        let marker_names: HashSet<&Component> = resolved
            .iter()
            .flat_map(|r| r.markers.iter().filter_map(|m| m.last()))
            .collect();
        let paths: Vec<Vec<Component>> = scanned
            .iter()
            .map(|e| components(&e.path, env.platform))
            .collect();
        let markers: HashSet<Vec<Component>> = paths
            .iter()
            .filter(|p| p.last().is_some_and(|name| marker_names.contains(name)))
            .cloned()
            .collect();

        let mut found = Vec::new();
        for (object, names) in scanned.iter().zip(&paths) {
            if !matches!(object.kind, EntryKind::File | EntryKind::Directory) {
                continue;
            }
            let ids: BTreeSet<&str> = resolved
                .iter()
                .filter(|r| r.matches(names, &markers))
                .map(|r| r.entry.id.as_str())
                .collect();
            for id in ids {
                let Ok(entry) = SourceName::new(id) else {
                    continue;
                };
                found.push(Evidence {
                    subject: Subject::File {
                        identity: object.identity().clone(),
                    },
                    fact: Fact::KnowledgeBaseMatch { entry },
                    basis: Basis::Observed,
                    provenance: Provenance {
                        source: source.clone(),
                        observed_at,
                    },
                });
            }
        }
        found
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

    const ENTRIES: &str = r#"{
        "id": "sample-cache",
        "tool": "Sample",
        "locations": [
            {"platforms": ["macos", "linux"], "root": {"kind": "home"}, "path": ".cache/sample"},
            {"platforms": ["macos"], "root": {"kind": "home"}, "path": "Library/Sample/*"},
            {"platforms": ["windows"], "root": {"kind": "local_app_data"}, "path": "Sample/cache"},
            {"platforms": ["macos", "linux", "windows"], "root": {"kind": "env_var", "name": "SAMPLE_CACHE"}, "path": ""}
        ],
        "safety_class": "regenerable",
        "regeneration": {"effort": "minutes", "network": true},
        "sources": ["https://example.com/docs"],
        "last_verified": "2026-10-14"
    }"#;

    const PROJECT: &str = r#"{
        "id": "sample-build",
        "tool": "Sample",
        "locations": [
            {"platforms": ["macos", "linux"], "root": {"kind": "project"}, "path": "target", "markers": ["Cargo.toml"]}
        ],
        "safety_class": "regenerable",
        "regeneration": {"effort": "minutes", "network": false},
        "sources": ["https://example.com/docs"],
        "last_verified": "2026-10-14"
    }"#;

    fn kb() -> KnowledgeBase {
        KnowledgeBase::load(&[
            ("sample-cache.json", ENTRIES),
            ("sample-build.json", PROJECT),
        ])
        .unwrap_or_else(|_| unreachable!())
    }

    fn unix(path: &str) -> RawPath {
        RawPath::from_unix_bytes(path.as_bytes().to_vec()).unwrap_or_else(|_| unreachable!())
    }

    fn windows(path: &str) -> RawPath {
        let wide: Vec<u16> = path.encode_utf16().collect();
        RawPath::from_windows_wide(&wide).unwrap_or_else(|_| unreachable!())
    }

    fn object(path: RawPath, kind: EntryKind, ino: u128) -> FilesystemEntry {
        FilesystemEntry {
            path,
            kind,
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

    fn dir(path: &str, ino: u128) -> FilesystemEntry {
        object(unix(path), EntryKind::Directory, ino)
    }

    fn env(platform: Platform) -> Environment {
        Environment {
            platform,
            home: Some(unix("/Users/ana")),
            local_app_data: None,
            roaming_app_data: None,
            vars: Vec::new(),
        }
    }

    /// `(file id, entry id)` of every match.
    fn matched(env: &Environment, scanned: &[FilesystemEntry]) -> Vec<(u128, String)> {
        kb().matches(env, scanned, Timestamp::UNIX_EPOCH)
            .into_iter()
            .filter_map(|e| match (e.subject, e.fact) {
                (Subject::File { identity }, Fact::KnowledgeBaseMatch { entry }) => {
                    Some((identity.file.get(), entry.as_str().to_owned()))
                }
                _ => None,
            })
            .collect()
    }

    #[test]
    fn home_locations_match_only_themselves() {
        let found = matched(
            &env(Platform::Macos),
            &[
                dir("/Users/ana/.cache/sample", 1),
                dir("/Users/ana/.cache/sample/sub", 2),
                dir("/Users/ana/.cache", 3),
                dir("/Users/bob/.cache/sample", 4),
                dir("/Users/ana/Library/Sample/one", 5),
                dir("/Users/ana/Library/Sample", 6),
                dir("/Users/ana/Library/Sample/one/two", 7),
            ],
        );
        assert_eq!(
            found,
            [
                (1, "sample-cache".to_owned()),
                (5, "sample-cache".to_owned())
            ]
        );
    }

    #[test]
    fn case_folds_only_where_the_platform_does() {
        let scanned = [dir("/Users/ana/.CACHE/Sample", 1)];
        assert_eq!(matched(&env(Platform::Macos), &scanned).len(), 1);
        assert_eq!(matched(&env(Platform::Linux), &scanned).len(), 0);
    }

    #[test]
    fn platforms_links_and_unset_roots_never_match() {
        let link = object(unix("/Users/ana/.cache/sample"), EntryKind::Symlink, 1);
        assert_eq!(matched(&env(Platform::Macos), &[link]).len(), 0, "links");
        let mac_only = [dir("/Users/ana/Library/Sample/x", 1)];
        assert_eq!(matched(&env(Platform::Linux), &mac_only).len(), 0);
        let mut homeless = env(Platform::Macos);
        homeless.home = None;
        assert_eq!(
            matched(&homeless, &[dir("/Users/ana/.cache/sample", 1)]).len(),
            0
        );
    }

    #[test]
    fn known_folders_and_variables_resolve() {
        let mut win = env(Platform::Windows);
        win.local_app_data = Some(windows(r"C:\Users\ana\AppData\Local"));
        win.vars = vec![("SAMPLE_CACHE".to_owned(), windows(r"D:\caches\sample"))];
        let found = matched(
            &win,
            &[
                object(
                    windows(r"c:\users\ana\appdata\local\SAMPLE\Cache"),
                    EntryKind::Directory,
                    1,
                ),
                object(windows(r"D:\caches\sample"), EntryKind::Directory, 2),
                object(windows(r"D:\caches"), EntryKind::Directory, 3),
            ],
        );
        assert_eq!(
            found,
            [
                (1, "sample-cache".to_owned()),
                (2, "sample-cache".to_owned())
            ]
        );
        let mut other = win.clone();
        other.vars = vec![("sample_cache".to_owned(), windows(r"D:\caches\sample"))];
        assert_eq!(
            matched(
                &other,
                &[object(
                    windows(r"D:\caches\sample"),
                    EntryKind::Directory,
                    2
                )]
            )
            .len(),
            0,
            "variable names are exact"
        );
    }

    /// An inner `*` stands for exactly one name: each Visual Studio
    /// instance's cache, never the instance folder or anything deeper.
    #[test]
    fn an_inner_wildcard_matches_one_name() {
        let builtin = KnowledgeBase::builtin().unwrap_or_else(|e| unreachable!("{e}"));
        let mut win = env(Platform::Windows);
        win.local_app_data = Some(windows(r"C:\Users\ana\AppData\Local"));
        let vs = r"C:\Users\ana\AppData\Local\Microsoft\VisualStudio";
        let scanned: Vec<FilesystemEntry> = [
            r"\17.0_1a2b3c4d\ComponentModelCache",
            r"\17.0_1a2b3c4d",
            r"\ComponentModelCache",
            r"\17.0_1a2b3c4d\Extensions\ComponentModelCache",
        ]
        .iter()
        .zip(1..)
        .map(|(tail, ino)| object(windows(&format!("{vs}{tail}")), EntryKind::Directory, ino))
        .collect();
        let found: Vec<u128> = builtin
            .matches(&win, &scanned, Timestamp::UNIX_EPOCH)
            .into_iter()
            .filter_map(|e| match e.subject {
                Subject::File { identity } => Some(identity.file.get()),
                _ => None,
            })
            .collect();
        assert_eq!(found, [1]);
    }

    #[test]
    fn projects_need_a_scanned_marker() {
        let found = matched(
            &env(Platform::Linux),
            &[
                dir("/src/app/target", 1),
                object(unix("/src/app/Cargo.toml"), EntryKind::File, 2),
                dir("/src/other/target", 3),
                dir("/target", 4),
                object(unix("/Cargo.toml"), EntryKind::File, 5),
            ],
        );
        assert_eq!(found, [(1, "sample-build".to_owned())]);
    }

    #[test]
    fn one_match_per_object_and_entry() {
        let mut both = env(Platform::Macos);
        both.vars = vec![("SAMPLE_CACHE".to_owned(), unix("/Users/ana/.cache/sample"))];
        assert_eq!(
            matched(&both, &[dir("/Users/ana/.cache/sample", 1)]).len(),
            1
        );
    }
}
