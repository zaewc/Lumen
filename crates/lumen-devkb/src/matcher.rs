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
//! - a project location matches only if one of its markers was scanned as a file:
//!   an unscanned marker is unknown, so no evidence is produced.
//!
//! [`KnowledgeBase::matches`] matches a whole scan at once. A [`Matcher`]
//! matches one in chunks with the same result: it gathers the scan's
//! [`Markers`] in a first pass and matches each chunk against them in a second.

use std::collections::{BTreeSet, HashSet};

use lumen_domain::{
    Basis, EntryKind, Evidence, Fact, FilesystemEntry, PathFlavor, Platform, Provenance, RawPath,
    SourceName, Subject, Timestamp,
};

use crate::{ArtifactKind, Entry, KnowledgeBase, Location, Root};

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
#[derive(Debug)]
struct Resolved<'kb> {
    entry: &'kb Entry,
    /// The absolute root, or `None` for a project location.
    root: Option<Vec<Component>>,
    pattern: Vec<Component>,
    markers: Vec<Vec<Component>>,
    kind: Option<ArtifactKind>,
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
        kind: location.kind,
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

/// Scanned paths that can serve as a project location's marker, gathered by
/// [`Matcher::add_markers`] from any number of scan chunks.
///
/// Only paths whose last name is one of the matcher's marker names are kept,
/// so the set stays small however large the scan is. Paths are folded for the
/// matcher's platform: use a set only with the [`Matcher`] that filled it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Markers {
    paths: HashSet<Vec<Component>>,
}

impl Markers {
    /// An empty set.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds every marker of `other`, such as one gathered from another chunk.
    pub fn merge(&mut self, other: Self) {
        self.paths.extend(other.paths);
    }

    /// The number of marker paths.
    #[must_use]
    pub fn len(&self) -> usize {
        self.paths.len()
    }

    /// Whether no marker path was found.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.paths.is_empty()
    }
}

/// A knowledge base resolved against one [`Environment`], ready to match scan
/// entries in chunks so a caller never needs the whole scan in memory.
///
/// A project location matches only if a marker beside it was scanned, and that
/// marker may be in any chunk. Matching a chunked scan therefore takes two
/// passes over it:
///
/// 1. [`Matcher::add_markers`] for every chunk, gathering one [`Markers`] set;
/// 2. [`Matcher::matches`] for every chunk against that complete set.
///
/// Evidence for a chunk is in the chunk's order, so concatenating the output
/// for consecutive chunks gives exactly what [`KnowledgeBase::matches`] gives
/// for the whole scan.
#[derive(Debug)]
pub struct Matcher<'kb> {
    platform: Platform,
    resolved: Vec<Resolved<'kb>>,
    /// The last names of all markers: the only paths a marker set keeps.
    marker_names: HashSet<Component>,
}

impl KnowledgeBase {
    /// This knowledge base's locations resolved against `env`; locations that
    /// do not apply there are left out.
    #[must_use]
    pub fn matcher(&self, env: &Environment) -> Matcher<'_> {
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
        let marker_names = resolved
            .iter()
            .flat_map(|r| r.markers.iter().filter_map(|m| m.last().cloned()))
            .collect();
        Matcher {
            platform: env.platform,
            resolved,
            marker_names,
        }
    }

    /// `KnowledgeBaseMatch` evidence for every scanned file or directory that is
    /// one of this knowledge base's locations in `env`, at most once per object
    /// and entry, in scan order. Markers are looked up in `scanned` itself; to
    /// match a scan in chunks, use [`KnowledgeBase::matcher`].
    pub fn matches(
        &self,
        env: &Environment,
        scanned: &[FilesystemEntry],
        observed_at: Timestamp,
    ) -> Vec<Evidence> {
        let matcher = self.matcher(env);
        let paths = matcher.paths(scanned);
        let mut markers = Markers::new();
        matcher.collect_markers(scanned, &paths, &mut markers);
        matcher.evidence(scanned, &paths, &markers, observed_at)
    }

    /// Whether entry `id`'s own command acts on `path`, an object that matched
    /// the entry in `env`. A tool uses the folder its variable names instead
    /// of its default one, so while any of the entry's variables is in `env`
    /// only their locations count, and only if they name one folder: which
    /// one the tool picks is otherwise unknown. An old default folder beside
    /// a configured one still matches the entry; its command just does not
    /// clear it.
    #[must_use]
    pub fn command_acts_on(&self, env: &Environment, id: &str, path: &RawPath) -> bool {
        let Some(entry) = self.get(id) else {
            return false;
        };
        let configured: Vec<Resolved<'_>> = entry
            .locations
            .iter()
            .filter(|l| matches!(l.root, Root::EnvVar { .. }))
            .filter_map(|l| resolve(entry, l, env))
            .collect();
        let folders: BTreeSet<Vec<Component>> = configured
            .iter()
            .map(|r| [r.root.as_deref().unwrap_or_default(), r.pattern.as_slice()].concat())
            .collect();
        let names = components(path, env.platform);
        configured.is_empty()
            || (folders.len() == 1
                && configured
                    .iter()
                    .any(|r| r.matches(&names, &HashSet::new())))
    }
}

impl Matcher<'_> {
    /// Pass 1: adds the marker paths among `chunk` to `markers`.
    pub fn add_markers(&self, chunk: &[FilesystemEntry], markers: &mut Markers) {
        self.collect_markers(chunk, &self.paths(chunk), markers);
    }

    /// Pass 2: `KnowledgeBaseMatch` evidence for every file or directory in
    /// `chunk` that is one of the knowledge base's locations, at most once per
    /// object and entry, in chunk order. `markers` must hold the markers of the
    /// whole scan, gathered by this matcher; a marker missing from it is
    /// unknown, so its project location does not match.
    #[must_use]
    pub fn matches(
        &self,
        chunk: &[FilesystemEntry],
        markers: &Markers,
        observed_at: Timestamp,
    ) -> Vec<Evidence> {
        self.evidence(chunk, &self.paths(chunk), markers, observed_at)
    }

    fn paths(&self, scanned: &[FilesystemEntry]) -> Vec<Vec<Component>> {
        scanned
            .iter()
            .map(|e| components(&e.path, self.platform))
            .collect()
    }

    fn collect_markers(
        &self,
        scanned: &[FilesystemEntry],
        paths: &[Vec<Component>],
        markers: &mut Markers,
    ) {
        markers.paths.extend(
            scanned
                .iter()
                .zip(paths)
                .filter(|(entry, path)| {
                    entry.kind == EntryKind::File
                        && path
                            .last()
                            .is_some_and(|name| self.marker_names.contains(name))
                })
                .map(|(_, path)| path.clone()),
        );
    }

    fn evidence(
        &self,
        scanned: &[FilesystemEntry],
        paths: &[Vec<Component>],
        markers: &Markers,
        observed_at: Timestamp,
    ) -> Vec<Evidence> {
        let Ok(source) = SourceName::new(SOURCE) else {
            return Vec::new();
        };
        let mut found = Vec::new();
        for (object, names) in scanned.iter().zip(paths) {
            if !matches!(object.kind, EntryKind::File | EntryKind::Directory) {
                continue;
            }
            let ids: BTreeSet<&str> = self
                .resolved
                .iter()
                .filter(|r| r.kind.is_none_or(|kind| kind.matches(object.kind)))
                .filter(|r| r.matches(names, &markers.paths))
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

    /// Paths below a macOS home and the built-in entry each one matches:
    /// Chromium's caches in known Electron apps' data folders match, and the
    /// rest of those folders (the user's and the sites' data) never does.
    const APP_FOLDERS: &[(&str, Option<&str>)] = &[
        (
            "Library/Application Support/Code/Cache",
            Some("vscode-chromium-caches"),
        ),
        (
            "Library/Application Support/Code/Code Cache",
            Some("vscode-chromium-caches"),
        ),
        (
            "Library/Application Support/Code/GPUCache",
            Some("vscode-chromium-caches"),
        ),
        (
            "Library/Application Support/Code/DawnGraphiteCache",
            Some("vscode-chromium-caches"),
        ),
        (
            "Library/Application Support/Code/DawnWebGPUCache",
            Some("vscode-chromium-caches"),
        ),
        (
            "library/application support/code/cache",
            Some("vscode-chromium-caches"),
        ),
        (
            "Library/Application Support/Code/Partitions/vscode-browser/Cache",
            Some("vscode-chromium-caches"),
        ),
        (
            "Library/Application Support/Code/Partitions/vscode-browser/Code Cache",
            Some("vscode-chromium-caches"),
        ),
        (
            "Library/Application Support/Code/CachedData",
            Some("vscode-cached-data"),
        ),
        (
            "Library/Application Support/Code/CachedExtensionVSIXs",
            Some("vscode-extension-vsix-cache"),
        ),
        ("Library/Application Support/Code", None),
        ("Library/Application Support/Code/User", None),
        ("Library/Application Support/Code/Local Storage", None),
        ("Library/Application Support/Code/Session Storage", None),
        ("Library/Application Support/Code/IndexedDB", None),
        ("Library/Application Support/Code/Service Worker", None),
        ("Library/Application Support/Code/Cookies", None),
        ("Library/Application Support/Code/logs", None),
        ("Library/Application Support/Code/Crashpad", None),
        ("Library/Application Support/Code/Backups", None),
        ("Library/Application Support/Code/Partitions", None),
        (
            "Library/Application Support/Code/Partitions/vscode-browser",
            None,
        ),
        (
            "Library/Application Support/Code/Partitions/vscode-browser/Local Storage",
            None,
        ),
        (
            "Library/Application Support/Code/Partitions/vscode-browser/Service Worker",
            None,
        ),
        (
            "Library/Application Support/Code/Partitions/a/b/Cache",
            None,
        ),
        ("Library/Application Support/Code/Cache/Cache_Data", None),
        ("Library/Application Support/Code/CachedData/0123abcd", None),
        ("Library/Application Support/Code - Insiders/Cache", None),
        (
            "Library/Application Support/Claude/Cache",
            Some("claude-desktop-chromium-caches"),
        ),
        (
            "Library/Application Support/Claude/Code Cache",
            Some("claude-desktop-chromium-caches"),
        ),
        (
            "Library/Application Support/Claude/GPUCache",
            Some("claude-desktop-chromium-caches"),
        ),
        (
            "Library/Application Support/Claude/Partitions/cowork-file-preview/GPUCache",
            Some("claude-desktop-chromium-caches"),
        ),
        ("Library/Application Support/Claude/Local Storage", None),
        ("Library/Application Support/Claude/IndexedDB", None),
        ("Library/Application Support/Claude/claude-code", None),
        (
            "Library/Application Support/Claude/claude-code-sessions",
            None,
        ),
        (
            "Library/Application Support/Claude/local-agent-mode-sessions",
            None,
        ),
        ("Library/Application Support/Claude/fcache", None),
        ("Library/Application Support/Claude/Partitions", None),
        (
            "Library/Application Support/Claude/vm_bundles/claudevm.bundle",
            None,
        ),
        (
            "Library/Application Support/discord/Cache",
            Some("discord-chromium-caches"),
        ),
        (
            "Library/Application Support/discord/Code Cache",
            Some("discord-chromium-caches"),
        ),
        (
            "Library/Application Support/discord/DawnWebGPUCache",
            Some("discord-chromium-caches"),
        ),
        ("Library/Application Support/discord/Local Storage", None),
        ("Library/Application Support/discord/IndexedDB", None),
        ("Library/Application Support/discord/Service Worker", None),
        ("Library/Application Support/discord/logs", None),
        (
            "Library/Application Support/discord/component_crx_cache",
            None,
        ),
        ("Library/Application Support/discord/download_cache", None),
        ("Library/Application Support/discord/modules", None),
        (
            "Library/Application Support/Notion/Cache",
            Some("notion-chromium-caches"),
        ),
        (
            "Library/Application Support/Notion/Partitions/notion/Cache",
            Some("notion-chromium-caches"),
        ),
        (
            "Library/Application Support/Notion/Partitions/notion/Code Cache",
            Some("notion-chromium-caches"),
        ),
        ("Library/Application Support/Notion/Partitions/notion", None),
        (
            "Library/Application Support/Notion/Partitions/notion/Service Worker",
            None,
        ),
        (
            "Library/Application Support/Notion/Partitions/notion/IndexedDB",
            None,
        ),
        (
            "Library/Application Support/Notion/Partitions/notion/File System",
            None,
        ),
        ("Library/Application Support/Notion/notion.db", None),
        (
            "Library/Application Support/Postman/Cache",
            Some("postman-chromium-caches"),
        ),
        (
            "Library/Application Support/Postman/Partitions/postman_shell/GPUCache",
            Some("postman-chromium-caches"),
        ),
        (
            "Library/Application Support/Postman/Partitions/084f8b0c-6958-4d74-887d-0a53512a19e5/Cache",
            Some("postman-chromium-caches"),
        ),
        (
            "Library/Application Support/Postman/Partitions/084f8b0c-6958-4d74-887d-0a53512a19e5/Service Worker",
            None,
        ),
        (
            "Library/Application Support/Postman/Partitions/postman_user_cookies_undefined",
            None,
        ),
        ("Library/Application Support/Postman/storage", None),
        (
            "Library/Application Support/obsidian/Cache",
            Some("obsidian-chromium-caches"),
        ),
        (
            "Library/Application Support/Obsidian/GPUCache",
            Some("obsidian-chromium-caches"),
        ),
        ("Library/Application Support/obsidian/IndexedDB", None),
        ("Library/Application Support/obsidian/obsidian.json", None),
        ("Library/Application Support/SomeApp/Cache", None),
        ("Library/Application Support/Spotify/PersistentCache", None),
        ("Library/Application Support/Claude/vm_bundles", None),
        ("Library/Caches/Code/Cache", None),
        ("Documents/Code/Cache", None),
    ];

    #[test]
    fn app_caches_match_and_app_data_never_does() {
        let builtin = KnowledgeBase::builtin().unwrap_or_else(|e| unreachable!("{e}"));
        let mac = env(Platform::Macos);
        for (tail, expected) in APP_FOLDERS {
            let scanned = [dir(&format!("/Users/ana/{tail}"), 1)];
            let found: Vec<String> = builtin
                .matches(&mac, &scanned, Timestamp::UNIX_EPOCH)
                .into_iter()
                .filter_map(|e| match e.fact {
                    Fact::KnowledgeBaseMatch { entry } => Some(entry.as_str().to_owned()),
                    _ => None,
                })
                .collect();
            let expected: Vec<String> = expected.iter().map(|id| (*id).to_owned()).collect();
            assert_eq!(found, expected, "{tail}");
        }
    }

    /// Where the vendor documents the folder on Windows and Linux, the same
    /// caches match there.
    #[test]
    fn app_caches_match_where_the_vendor_documents_the_folder() {
        let builtin = KnowledgeBase::builtin().unwrap_or_else(|e| unreachable!("{e}"));
        let ids = |env: &Environment, scanned: &[FilesystemEntry]| -> Vec<String> {
            builtin
                .matches(env, scanned, Timestamp::UNIX_EPOCH)
                .into_iter()
                .filter_map(|e| match e.fact {
                    Fact::KnowledgeBaseMatch { entry } => Some(entry.as_str().to_owned()),
                    _ => None,
                })
                .collect()
        };
        let mut win = env(Platform::Windows);
        win.roaming_app_data = Some(windows(r"C:\Users\ana\AppData\Roaming"));
        let roaming = |tail: &str| {
            object(
                windows(&format!(r"C:\Users\ana\AppData\Roaming\{tail}")),
                EntryKind::Directory,
                1,
            )
        };
        assert_eq!(
            ids(
                &win,
                &[
                    roaming(r"Code\GPUCache"),
                    roaming(r"Code\CachedExtensionVSIXs")
                ]
            ),
            ["vscode-chromium-caches", "vscode-extension-vsix-cache"]
        );
        assert_eq!(ids(&win, &[roaming(r"Code\Local Storage")]).len(), 0);
        assert_eq!(
            ids(
                &win,
                &[
                    roaming(r"discord\Cache"),
                    roaming(r"Discord\Code Cache"),
                    roaming(r"discord\DawnWebGPUCache"),
                    roaming(r"Postman\Code Cache"),
                    roaming(r"Postman\Partitions\postman_shell\GPUCache"),
                    roaming(r"Postman\Partitions\postman_user_cookies_undefined\Cache"),
                ]
            ),
            [
                "discord-chromium-caches",
                "discord-chromium-caches",
                "discord-chromium-caches",
                "postman-chromium-caches",
                "postman-chromium-caches",
                "postman-chromium-caches",
            ]
        );
        assert_eq!(
            ids(
                &win,
                &[
                    roaming("discord"),
                    roaming(r"discord\Local Storage"),
                    roaming(r"discord\IndexedDB"),
                    roaming(r"discord\Service Worker"),
                    roaming(r"discord\module_data"),
                    roaming(r"discord\Network"),
                    roaming(r"discord\Cache\Cache_Data"),
                    roaming(r"Postman\storage"),
                    roaming(r"Postman\Local Storage"),
                    roaming(r"Postman\Partitions"),
                    roaming(r"Postman\Partitions\postman_user_cookies_undefined"),
                    roaming(r"Postman\Partitions\postman_user_cookies_undefined\Network"),
                    roaming(r"Postman\Partitions\postman_shell\Service Worker"),
                ]
            )
            .len(),
            0
        );
        let linux = env(Platform::Linux);
        assert_eq!(
            ids(&linux, &[dir("/Users/ana/.config/Code/Code Cache", 1)]),
            ["vscode-chromium-caches"]
        );
        assert_eq!(
            ids(&linux, &[dir("/Users/ana/.config/code/Cache", 1)]).len(),
            0,
            "Linux names are exact"
        );
    }

    /// pip's cache matches at its default on each platform, under
    /// `XDG_CACHE_HOME` and at `PIP_CACHE_DIR`; never its parent or contents.
    #[test]
    fn pip_cache_matches_where_pip_documents_it() {
        let builtin = KnowledgeBase::builtin().unwrap_or_else(|e| unreachable!("{e}"));
        let ids = |env: &Environment, scanned: &[FilesystemEntry]| -> Vec<String> {
            builtin
                .matches(env, scanned, Timestamp::UNIX_EPOCH)
                .into_iter()
                .filter_map(|e| match e.fact {
                    Fact::KnowledgeBaseMatch { entry } => Some(entry.as_str().to_owned()),
                    _ => None,
                })
                .collect()
        };
        let mut linux = env(Platform::Linux);
        linux.vars = vec![("XDG_CACHE_HOME".to_owned(), unix("/xdg"))];
        assert_eq!(
            ids(
                &linux,
                &[dir("/Users/ana/.cache/pip", 1), dir("/xdg/pip", 2)]
            ),
            ["pip-cache", "pip-cache"]
        );
        assert_eq!(
            ids(
                &linux,
                &[
                    dir("/Users/ana/.cache/Pip", 1),
                    dir("/Users/ana/.cache/pip/http-v2", 2),
                    dir("/Users/ana/Library/Caches/pip", 3),
                ]
            )
            .len(),
            0
        );
        let mut mac = env(Platform::Macos);
        mac.vars = linux.vars.clone();
        assert_eq!(
            ids(
                &mac,
                &[dir("/Users/ana/Library/Caches/pip", 1), dir("/xdg/pip", 2)]
            ),
            ["pip-cache", "pip-cache"]
        );
        assert_eq!(
            ids(&mac, &[dir("/Users/ana/.cache/pip", 1)]).len(),
            0,
            "macOS uses ~/.cache only through XDG_CACHE_HOME"
        );
        let mut win = env(Platform::Windows);
        win.local_app_data = Some(windows(r"C:\Users\ana\AppData\Local"));
        win.vars = vec![("PIP_CACHE_DIR".to_owned(), windows(r"D:\pip"))];
        let local = |tail: &str, ino| {
            object(
                windows(&format!(r"C:\Users\ana\AppData\Local\{tail}")),
                EntryKind::Directory,
                ino,
            )
        };
        assert_eq!(
            ids(
                &win,
                &[
                    local(r"pip\Cache", 1),
                    local(r"pip\cache", 2),
                    object(windows(r"D:\pip"), EntryKind::Directory, 3),
                ]
            ),
            ["pip-cache", "pip-cache", "pip-cache"]
        );
        assert_eq!(
            ids(&win, &[local("pip", 1), local(r"pip\Cache\wheels", 2)]).len(),
            0
        );
    }

    #[test]
    fn npx_cache_stays_separate_from_packages_and_the_download_cache() {
        let kb = KnowledgeBase::builtin().unwrap_or_else(|e| unreachable!("{e}"));
        for platform in [Platform::Macos, Platform::Linux, Platform::Windows] {
            let (base, custom) = if platform == Platform::Windows {
                ("C:/Users/ana/AppData/Local/npm-cache", "D:/cache")
            } else {
                ("/Users/ana/.npm", "/custom/npm")
            };
            let parse = |path: &str| {
                if platform == Platform::Windows {
                    windows(&path.replace('/', "\\"))
                } else {
                    unix(path)
                }
            };
            let join = |root: &str, tail: &str| parse(&format!("{root}/{tail}"));
            let target = join(base, "_npx");
            let custom_target = join(custom, "_npx");
            let npx_matches = |environment: &Environment, entries: &[FilesystemEntry]| {
                kb.matches(environment, entries, Timestamp::UNIX_EPOCH)
                    .into_iter()
                    .filter(|e| matches!(&e.fact, Fact::KnowledgeBaseMatch { entry } if entry.as_str() == "npm-npx-cache"))
                    .collect::<Vec<_>>()
            };
            let mut environment = env(platform);
            environment.local_app_data = Some(windows(r"C:\Users\ana\AppData\Local"));
            let scanned = [
                object(target.clone(), EntryKind::Directory, 1),
                object(join(base, "_cacache"), EntryKind::Directory, 2),
                object(parse(base), EntryKind::Directory, 3),
                object(join(base, "_npx/package"), EntryKind::Directory, 4),
                object(
                    join(custom, "project/node_modules"),
                    EntryKind::Directory,
                    7,
                ),
            ];
            let found = npx_matches(&environment, &scanned);
            assert_eq!(found.len(), 1, "{platform:?}");
            assert_eq!(
                found[0].subject,
                Subject::File {
                    identity: scanned[0].identity().clone()
                }
            );
            assert!(kb.command_acts_on(&environment, "npm-npx-cache", &target));
            for kind in [
                EntryKind::File,
                EntryKind::Symlink,
                EntryKind::Junction,
                EntryKind::OtherReparsePoint,
            ] {
                assert_eq!(
                    npx_matches(&environment, &[object(target.clone(), kind, 5)]),
                    [],
                    "{platform:?} {kind:?}"
                );
            }
            for variable in ["npm_config_cache", "NPM_CONFIG_CACHE"] {
                environment.vars = vec![(variable.into(), parse(custom))];
                let configured = object(custom_target.clone(), EntryKind::Directory, 6);
                assert_eq!(
                    npx_matches(&environment, std::slice::from_ref(&configured)).len(),
                    1
                );
                assert!(kb.command_acts_on(&environment, "npm-npx-cache", &custom_target));
                assert!(!kb.command_acts_on(&environment, "npm-npx-cache", &target));
            }
            environment.vars = vec![
                ("npm_config_cache".into(), parse(base)),
                ("NPM_CONFIG_CACHE".into(), parse(custom)),
            ];
            assert!(!kb.command_acts_on(&environment, "npm-npx-cache", &target));
            assert!(!kb.command_acts_on(&environment, "npm-npx-cache", &custom_target));
        }
    }

    /// Brave's caches match in the user cache directory Chromium derives from
    /// Brave's `BraveSoftware/Brave-Browser` user data directory; nothing in
    /// a profile, no other channel and no Windows location ever matches.
    #[test]
    fn brave_caches_match_only_in_its_user_cache_directory() {
        let builtin = KnowledgeBase::builtin().unwrap_or_else(|e| unreachable!("{e}"));
        let ids = |env: &Environment, scanned: &[FilesystemEntry]| -> Vec<String> {
            builtin
                .matches(env, scanned, Timestamp::UNIX_EPOCH)
                .into_iter()
                .filter_map(|e| match e.fact {
                    Fact::KnowledgeBaseMatch { entry } => Some(entry.as_str().to_owned()),
                    _ => None,
                })
                .collect()
        };
        let mac = env(Platform::Macos);
        let caches = "/Users/ana/Library/Caches/BraveSoftware/Brave-Browser";
        assert_eq!(
            ids(
                &mac,
                &[
                    dir(&format!("{caches}/Default/Cache"), 1),
                    dir(&format!("{caches}/Profile 1/Code Cache"), 2),
                ]
            ),
            ["brave-browser-caches"; 2]
        );
        let support = "/Users/ana/Library/Application Support/BraveSoftware/Brave-Browser";
        assert_eq!(
            ids(
                &mac,
                &[
                    dir(caches, 1),
                    dir(&format!("{caches}/Default"), 2),
                    dir(&format!("{caches}/Default/Cache/Cache_Data"), 3),
                    dir(&format!("{support}/Default/Cache"), 4),
                    dir(&format!("{support}/Default/Cookies"), 5),
                    dir(&format!("{support}/Default/IndexedDB"), 6),
                    dir(
                        "/Users/ana/Library/Caches/BraveSoftware/Brave-Browser-Beta/Default/Cache",
                        7
                    ),
                ]
            )
            .len(),
            0
        );
        let mut linux = env(Platform::Linux);
        linux.vars = vec![("XDG_CACHE_HOME".to_owned(), unix("/xdg"))];
        assert_eq!(
            ids(
                &linux,
                &[
                    dir(
                        "/Users/ana/.cache/BraveSoftware/Brave-Browser/Default/Cache",
                        1
                    ),
                    dir("/xdg/BraveSoftware/Brave-Browser/Default/Code Cache", 2),
                ]
            ),
            ["brave-browser-caches"; 2]
        );
        assert_eq!(
            ids(
                &linux,
                &[
                    dir(
                        "/Users/ana/.config/BraveSoftware/Brave-Browser/Default/Cache",
                        1
                    ),
                    dir(
                        "/Users/ana/.cache/bravesoftware/brave-browser/Default/Cache",
                        2
                    ),
                ]
            )
            .len(),
            0
        );
        let mut win = env(Platform::Windows);
        win.local_app_data = Some(windows(r"C:\Users\ana\AppData\Local"));
        assert_eq!(
            ids(
                &win,
                &[object(
                    windows(
                        r"C:\Users\ana\AppData\Local\BraveSoftware\Brave-Browser\User Data\Default\Cache"
                    ),
                    EntryKind::Directory,
                    1,
                )]
            )
            .len(),
            0,
            "Brave's Windows caches are inside its profile"
        );
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
    fn only_regular_files_qualify_as_project_markers() {
        for kind in [
            EntryKind::Directory,
            EntryKind::Symlink,
            EntryKind::Junction,
            EntryKind::OtherReparsePoint,
            EntryKind::Special,
        ] {
            let scanned = [
                dir("/src/app/target", 1),
                object(unix("/src/app/Cargo.toml"), kind, 2),
            ];
            let linux = env(Platform::Linux);
            assert!(matched(&linux, &scanned).is_empty(), "{kind:?}");
            let (target, marker) = scanned.split_at(1);
            assert_eq!(chunked(&kb(), &linux, &[target, marker]), Vec::new());
        }
    }

    #[test]
    fn nextjs_matches_only_a_projects_build_cache() {
        let builtin = KnowledgeBase::builtin().unwrap_or_else(|e| unreachable!("{e}"));
        for platform in [Platform::Macos, Platform::Linux, Platform::Windows] {
            let path = |tail: &str| {
                if platform == Platform::Windows {
                    windows(&format!(r"C:\src\{tail}").replace('/', "\\"))
                } else {
                    unix(&format!("/src/{tail}"))
                }
            };
            let scanned = [
                object(path("app/.next/cache"), EntryKind::Directory, 1),
                object(path("app/package.json"), EntryKind::File, 2),
                object(path("app/.next"), EntryKind::Directory, 3),
                object(path("app/.next/server"), EntryKind::Directory, 4),
                object(path("app/.next/static"), EntryKind::Directory, 5),
                object(path("app/node_modules"), EntryKind::Directory, 6),
                object(path("app/src"), EntryKind::Directory, 7),
                object(path("other/.next/cache"), EntryKind::Directory, 8),
                object(path("app/.next/cache/webpack"), EntryKind::Directory, 9),
            ];
            let environment = env(platform);
            let found = builtin.matches(&environment, &scanned, Timestamp::UNIX_EPOCH);
            assert_eq!(found.len(), 1, "{platform:?}");
            assert_eq!(
                found[0].subject,
                Subject::File {
                    identity: scanned[0].identity().clone()
                }
            );
            assert!(matches!(
                &found[0].fact,
                Fact::KnowledgeBaseMatch { entry } if entry.as_str() == "nextjs-build-cache"
            ));
            let (cache, rest) = scanned.split_at(1);
            assert_eq!(chunked(&builtin, &environment, &[cache, rest]), found);
        }
    }

    #[test]
    fn optional_location_kind_narrows_only_that_location() -> Result<(), Box<dyn std::error::Error>>
    {
        let environment = env(Platform::Linux);
        for (required, file, directory) in [
            (None, true, true),
            (Some(ArtifactKind::File), true, false),
            (Some(ArtifactKind::Directory), false, true),
        ] {
            let mut entry: Entry = serde_json::from_str(ENTRIES)?;
            entry.locations.truncate(1);
            entry.locations[0].kind = required;
            let json = serde_json::to_string(&entry)?;
            let kb = KnowledgeBase::load(&[("sample-cache.json", &json)])?;
            for actual in [
                EntryKind::File,
                EntryKind::Directory,
                EntryKind::Symlink,
                EntryKind::Special,
            ] {
                let scanned = [object(unix("/Users/ana/.cache/sample"), actual, 1)];
                let expected = match actual {
                    EntryKind::File => file,
                    EntryKind::Directory => directory,
                    _ => false,
                };
                assert_eq!(
                    !kb.matches(&environment, &scanned, Timestamp::UNIX_EPOCH)
                        .is_empty(),
                    expected,
                    "{required:?}, {actual:?}"
                );
            }
        }
        Ok(())
    }

    #[test]
    fn cargo_matches_intermediates_but_not_deliverables() {
        let builtin = KnowledgeBase::builtin().unwrap_or_else(|e| unreachable!("{e}"));
        for platform in [Platform::Macos, Platform::Linux, Platform::Windows] {
            let path = |tail: &str| {
                if platform == Platform::Windows {
                    windows(&format!(r"C:\src\{tail}").replace('/', "\\"))
                } else {
                    unix(&format!("/src/{tail}"))
                }
            };
            let mut scanned = Vec::new();
            for (index, tail) in [
                "app/target/debug/deps",
                "app/target/debug/build",
                "app/target/debug/incremental",
                "app/target/release/deps",
                "app/target/release/build",
                "app/target/release/incremental",
                "app/target",
                "app/target/debug",
                "app/target/release",
                "app/target/debug/examples",
                "app/target/doc",
                "app/target/package",
                "app/target/release/bundle",
                "app/src",
                "other/target/debug/deps",
                "app/target/debug/deps/nested",
            ]
            .iter()
            .enumerate()
            {
                scanned.push(object(path(tail), EntryKind::Directory, index as u128 + 1));
            }
            scanned.push(object(path("app/target/release/app"), EntryKind::File, 50));
            scanned.push(object(path("app/Cargo.toml"), EntryKind::File, 51));
            let environment = env(platform);
            let found = builtin.matches(&environment, &scanned, Timestamp::UNIX_EPOCH);
            assert_eq!(found.len(), 6, "{platform:?}");
            for (evidence, cache) in found.iter().zip(&scanned[..6]) {
                assert_eq!(
                    evidence.subject,
                    Subject::File {
                        identity: cache.identity().clone()
                    }
                );
                assert!(matches!(
                    &evidence.fact,
                    Fact::KnowledgeBaseMatch { entry } if entry.as_str() == "cargo-project-build-cache"
                ));
            }
            let (caches, rest) = scanned.split_at(6);
            assert_eq!(chunked(&builtin, &environment, &[caches, rest]), found);
        }
    }

    #[test]
    fn cargo_requires_a_regular_manifest_and_a_real_cache_directory() {
        let builtin = KnowledgeBase::builtin().unwrap_or_else(|e| unreachable!("{e}"));
        let environment = env(Platform::Linux);
        let cache = dir("/src/app/target/debug/deps", 1);
        assert_eq!(
            builtin.matches(
                &environment,
                std::slice::from_ref(&cache),
                Timestamp::UNIX_EPOCH
            ),
            []
        );
        for kind in [
            EntryKind::Directory,
            EntryKind::Symlink,
            EntryKind::Junction,
            EntryKind::OtherReparsePoint,
        ] {
            let marker = object(unix("/src/app/Cargo.toml"), kind, 2);
            assert_eq!(
                builtin.matches(
                    &environment,
                    &[cache.clone(), marker],
                    Timestamp::UNIX_EPOCH
                ),
                []
            );
        }
        for kind in [
            EntryKind::File,
            EntryKind::Symlink,
            EntryKind::Junction,
            EntryKind::OtherReparsePoint,
        ] {
            let cache = object(unix("/src/app/target/debug/deps"), kind, 1);
            let marker = object(unix("/src/app/Cargo.toml"), EntryKind::File, 2);
            assert_eq!(
                builtin.matches(&environment, &[cache, marker], Timestamp::UNIX_EPOCH),
                []
            );
        }
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

    /// Two-pass matching of `chunks`: every chunk's markers first, then every
    /// chunk's evidence against them, concatenated.
    fn chunked(
        kb: &KnowledgeBase,
        env: &Environment,
        chunks: &[&[FilesystemEntry]],
    ) -> Vec<Evidence> {
        let matcher = kb.matcher(env);
        let mut markers = Markers::new();
        for chunk in chunks {
            matcher.add_markers(chunk, &mut markers);
        }
        chunks
            .iter()
            .flat_map(|chunk| matcher.matches(chunk, &markers, Timestamp::UNIX_EPOCH))
            .collect()
    }

    #[test]
    fn a_marker_in_another_chunk_still_qualifies_its_project() {
        let kb = kb();
        let linux = env(Platform::Linux);
        let scanned = [
            dir("/src/app/target", 1),
            dir("/src/app/src", 2),
            object(unix("/src/app/Cargo.toml"), EntryKind::File, 3),
        ];
        let whole = kb.matches(&linux, &scanned, Timestamp::UNIX_EPOCH);
        assert_eq!(whole.len(), 1);
        let (first, second) = scanned.split_at(2);
        assert!(
            kb.matches(&linux, first, Timestamp::UNIX_EPOCH).is_empty(),
            "one pass over a chunk cannot see the marker"
        );
        assert_eq!(chunked(&kb, &linux, &[first, second]), whole);
        // Markers gathered per chunk merge into the same set.
        let matcher = kb.matcher(&linux);
        let mut merged = Markers::new();
        for chunk in [second, first] {
            let mut part = Markers::new();
            matcher.add_markers(chunk, &mut part);
            merged.merge(part);
        }
        assert_eq!(merged.len(), 1, "only marker paths are kept");
        assert_eq!(
            [first, second]
                .iter()
                .flat_map(|c| matcher.matches(c, &merged, Timestamp::UNIX_EPOCH))
                .collect::<Vec<_>>(),
            whole
        );
    }

    /// Any chunking of a scan gives exactly the evidence, in the same order,
    /// that matching the whole scan at once gives.
    #[test]
    fn every_chunking_matches_like_the_whole_scan() {
        let kb = kb();
        let mut mac = env(Platform::Macos);
        mac.vars = vec![("SAMPLE_CACHE".to_owned(), unix("/Users/ana/.cache/sample"))];
        let scanned = [
            dir("/Users/ana/src/one/target", 1),
            dir("/Users/ana/.cache/sample", 2),
            object(unix("/Users/ana/src/two/Cargo.toml"), EntryKind::File, 3),
            dir("/Users/ana/Library/Sample/a", 4),
            dir("/Users/ana/src/two/TARGET", 5),
            object(unix("/Users/ana/src/one/cargo.toml"), EntryKind::File, 6),
            dir("/Users/ana/src/three/target", 7),
            object(unix("/Users/ana/Library/Sample/b"), EntryKind::Symlink, 8),
            dir("/Users/ana/Library/Sample/c", 9),
        ];
        let whole = kb.matches(&mac, &scanned, Timestamp::UNIX_EPOCH);
        let ids: Vec<u128> = whole
            .iter()
            .filter_map(|e| match &e.subject {
                Subject::File { identity } => Some(identity.file.get()),
                _ => None,
            })
            .collect();
        assert_eq!(ids, [1, 2, 4, 5, 9], "the fixture covers every rule");
        for size in 1..=scanned.len() {
            let chunks: Vec<&[FilesystemEntry]> = scanned.chunks(size).collect();
            assert_eq!(chunked(&kb, &mac, &chunks), whole, "chunks of {size}");
        }
        // Uneven chunkings, including empty chunks.
        for cuts in [&[0, 0, 9][..], &[1, 6], &[3, 3, 8], &[2, 5, 7]] {
            let mut chunks = Vec::new();
            let mut start = 0;
            for &cut in cuts.iter().chain([&scanned.len()]) {
                chunks.push(&scanned[start..cut]);
                start = cut;
            }
            assert_eq!(chunked(&kb, &mac, &chunks), whole, "cuts at {cuts:?}");
        }
    }
}
