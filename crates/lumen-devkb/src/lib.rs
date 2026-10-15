//! Lumen's developer-artifact knowledge base (ADR-0021).
//!
//! Each [`Entry`] describes one tool's artifact: where it lives, how safe it is
//! to remove, what regenerating it costs, the tool's own cleanup command, and the
//! official sources it was verified against. Entries are JSON files under
//! `entries/`, embedded at build time and validated on load; the format's JSON
//! Schema is `schemas/devkb-entry.schema.json`.
//!
//! The knowledge base is evidence for the safety policy, never a decision: an
//! entry can say an artifact is regenerable, but only the policy chooses an
//! action. This crate is IO-free.

#![forbid(unsafe_code)]

mod matcher;
mod validate;

use std::collections::{BTreeMap, BTreeSet, HashSet};

use lumen_domain::{Platform, SourceName, Timestamp};
use lumen_policy::KnowledgeClass;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub use matcher::{Environment, SOURCE};
pub use validate::Problem;

/// The built-in entries as `(file name, JSON)`, one per tool (roadmap 6.15+).
const BUILTIN: &[(&str, &str)] = &[
    (
        "cargo-global-cache.json",
        include_str!("../entries/cargo-global-cache.json"),
    ),
    (
        "claude-code-session-data.json",
        include_str!("../entries/claude-code-session-data.json"),
    ),
    (
        "go-build-cache.json",
        include_str!("../entries/go-build-cache.json"),
    ),
    (
        "go-module-cache.json",
        include_str!("../entries/go-module-cache.json"),
    ),
    (
        "gradle-user-home-caches.json",
        include_str!("../entries/gradle-user-home-caches.json"),
    ),
    (
        "homebrew-download-cache.json",
        include_str!("../entries/homebrew-download-cache.json"),
    ),
    ("npm-cache.json", include_str!("../entries/npm-cache.json")),
    (
        "pnpm-store.json",
        include_str!("../entries/pnpm-store.json"),
    ),
    ("uv-cache.json", include_str!("../entries/uv-cache.json")),
    (
        "xcode-derived-data.json",
        include_str!("../entries/xcode-derived-data.json"),
    ),
    (
        "xcode-ios-device-support.json",
        include_str!("../entries/xcode-ios-device-support.json"),
    ),
];

/// One artifact of one developer tool.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    /// Stable identifier, also the file name (`<id>.json`): lowercase letters,
    /// digits and `-`.
    #[schemars(regex(pattern = r"^[a-z][a-z0-9-]{0,62}[a-z0-9]$"))]
    pub id: String,
    /// The tool's display name, e.g. `Cargo`.
    #[schemars(length(min = 1, max = 64))]
    pub tool: String,
    /// Where the artifact lives, per platform.
    #[schemars(length(min = 1))]
    pub locations: Vec<Location>,
    /// How the artifact may be cleaned.
    pub safety_class: SafetyClass,
    /// What removing it costs.
    pub regeneration: Regeneration,
    /// The tool's own cleanup command, if it has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub official_action: Option<ToolAction>,
    /// Official documentation the entry was verified against (`https` only).
    #[schemars(length(min = 1))]
    pub sources: Vec<String>,
    /// The day the entry was last checked against its sources (`YYYY-MM-DD`).
    #[schemars(regex(pattern = r"^\d{4}-\d{2}-\d{2}$"))]
    pub last_verified: String,
}

/// One place an artifact lives.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Location {
    /// Platforms this location applies to.
    #[schemars(length(min = 1))]
    pub platforms: BTreeSet<Platform>,
    /// The directory `path` is relative to.
    pub root: Root,
    /// `/`-separated path below the root; a `*` component matches any one name.
    /// Empty only for an `env_var` root (the variable names the artifact).
    pub path: String,
    /// Files relative to the root, any one of which must exist for the location
    /// to match (e.g. a lockfile). Required for a `project` root.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub markers: Vec<String>,
}

/// A directory known per user or per project.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Root {
    /// The user's home directory.
    Home,
    /// Windows `%LOCALAPPDATA%` (`FOLDERID_LocalAppData`).
    LocalAppData,
    /// Windows `%APPDATA%` (`FOLDERID_RoamingAppData`).
    RoamingAppData,
    /// A directory named by an environment variable the tool documents (e.g.
    /// `GOMODCACHE`); a location only applies when the variable is set.
    EnvVar {
        /// The variable: ASCII letters, digits and `_`, not starting with a
        /// digit. Matched exactly as written: Unix names are case-sensitive, so
        /// a tool that reads both cases (e.g. `npm_config_cache`) lists both.
        #[schemars(regex(pattern = r"^[A-Za-z_][A-Za-z0-9_]{0,63}$"))]
        name: String,
    },
    /// A project directory, recognised by the location's markers.
    Project,
}

/// How an artifact may be cleaned (ADR-0021).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum SafetyClass {
    /// The tool cleans it itself; informational only.
    SelfManaged,
    /// Only the tool's own command may clean it.
    ToolAction,
    /// May be quarantined; the tool regenerates it.
    Regenerable,
    /// Cannot be regenerated: `REVIEW` only, never an action.
    NotRegenerable,
}

impl From<SafetyClass> for KnowledgeClass {
    fn from(class: SafetyClass) -> Self {
        match class {
            SafetyClass::SelfManaged => Self::SelfManaged,
            SafetyClass::ToolAction => Self::ToolAction,
            SafetyClass::Regenerable => Self::Regenerable,
            SafetyClass::NotRegenerable => Self::NotRegenerable,
        }
    }
}

/// What regenerating an artifact costs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Regeneration {
    /// Rough time to regenerate.
    pub effort: Effort,
    /// Whether regenerating downloads from the network.
    pub network: bool,
}

/// Rough time to regenerate an artifact.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum Effort {
    /// Seconds.
    Seconds,
    /// Minutes.
    Minutes,
    /// Hours.
    Hours,
    /// It cannot be regenerated.
    Impossible,
}

/// A tool's own cleanup command, as a literal argument vector: never a shell
/// string. The program is a bare name that the runner resolves from known
/// install locations, never from `PATH` (ADR-0021).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ToolAction {
    /// The program's bare name, e.g. `cargo`.
    #[schemars(regex(pattern = r"^[A-Za-z0-9][A-Za-z0-9._+-]{0,63}$"))]
    pub program: String,
    /// Arguments that perform the cleanup.
    pub args: Vec<String>,
    /// Arguments that only preview it, if the tool has a dry run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dry_run_args: Option<Vec<String>>,
}

impl Entry {
    /// The day the entry was last verified, as midnight UTC.
    pub fn verified_on(&self) -> Option<Timestamp> {
        validate::day(&self.last_verified)
    }
}

/// Why the knowledge base could not be loaded.
#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    /// A file is not a well-formed entry.
    #[error("{file}: not a knowledge-base entry: {source}")]
    Parse {
        /// The file name.
        file: String,
        /// The JSON error.
        #[source]
        source: serde_json::Error,
    },
    /// An entry breaks a rule.
    #[error("{file}: {problem}")]
    Invalid {
        /// The file name.
        file: String,
        /// The broken rule.
        problem: Problem,
    },
    /// A file is not named after its entry's id.
    #[error("{file}: must be named `{id}.json`")]
    FileName {
        /// The file name.
        file: String,
        /// The entry's id.
        id: String,
    },
    /// Two entries share an id.
    #[error("duplicate entry id `{0}`")]
    Duplicate(String),
}

/// An entry whose `last_verified` needs attention.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DateIssue {
    /// Verified longer ago than allowed.
    Stale {
        /// Whole days since verification.
        days: i64,
    },
    /// Verified "after" today: a typo, not a verification.
    InFuture,
}

/// A validated set of entries with unique ids.
#[derive(Debug, Clone, Default)]
pub struct KnowledgeBase {
    entries: Vec<Entry>,
}

impl KnowledgeBase {
    /// Parses and validates `(file name, JSON)` pairs.
    ///
    /// # Errors
    ///
    /// Returns the first [`LoadError`]: a malformed or invalid entry, a file not
    /// named `<id>.json`, or a duplicate id.
    pub fn load(files: &[(&str, &str)]) -> Result<Self, LoadError> {
        let mut entries = Vec::with_capacity(files.len());
        let mut ids = HashSet::new();
        for (file, json) in files {
            let entry: Entry = serde_json::from_str(json).map_err(|source| LoadError::Parse {
                file: (*file).to_owned(),
                source,
            })?;
            validate::entry(&entry).map_err(|problem| LoadError::Invalid {
                file: (*file).to_owned(),
                problem,
            })?;
            if *file != format!("{}.json", entry.id) {
                return Err(LoadError::FileName {
                    file: (*file).to_owned(),
                    id: entry.id,
                });
            }
            if !ids.insert(entry.id.clone()) {
                return Err(LoadError::Duplicate(entry.id));
            }
            entries.push(entry);
        }
        Ok(Self { entries })
    }

    /// The entries shipped with Lumen.
    ///
    /// # Errors
    ///
    /// Returns a [`LoadError`] if an embedded entry is invalid; the tests make
    /// sure none is.
    pub fn builtin() -> Result<Self, LoadError> {
        Self::load(BUILTIN)
    }

    /// The entries, in load order.
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// The entry with `id`.
    pub fn get(&self, id: &str) -> Option<&Entry> {
        self.entries.iter().find(|e| e.id == id)
    }

    /// Every entry's safety class as the policy sees it, by entry ID: the
    /// `PolicyInput.knowledge` map (roadmap 10.5). Entry IDs are valid source
    /// names by construction (`[a-z][a-z0-9-]*`), the same names
    /// `KnowledgeBaseMatch` evidence carries.
    pub fn knowledge(&self) -> BTreeMap<SourceName, KnowledgeClass> {
        self.entries
            .iter()
            .filter_map(|e| Some((SourceName::new(&e.id).ok()?, e.safety_class.into())))
            .collect()
    }

    /// Entries verified more than `max_age_days` before `today`, or after it.
    pub fn date_issues(&self, today: Timestamp, max_age_days: i64) -> Vec<(&Entry, DateIssue)> {
        self.entries
            .iter()
            .filter_map(|entry| {
                let days = today.whole_days_since(entry.verified_on()?);
                if days < 0 {
                    Some((entry, DateIssue::InFuture))
                } else if days > max_age_days {
                    Some((entry, DateIssue::Stale { days }))
                } else {
                    None
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(crate) const CARGO: &str = r#"{
        "id": "cargo-registry",
        "tool": "Cargo",
        "locations": [
            {"platforms": ["macos", "linux"], "root": {"kind": "home"}, "path": ".cargo/registry"},
            {"platforms": ["macos", "linux", "windows"], "root": {"kind": "env_var", "name": "CARGO_HOME"}, "path": "registry"}
        ],
        "safety_class": "self_managed",
        "regeneration": {"effort": "minutes", "network": true},
        "sources": ["https://blog.rust-lang.org/2025/06/26/Rust-1.88.0/"],
        "last_verified": "2026-10-05"
    }"#;

    fn day(text: &str) -> Timestamp {
        validate::day(text).unwrap_or_else(|| unreachable!())
    }

    #[test]
    fn the_builtin_entries_load() {
        assert!(KnowledgeBase::builtin().is_ok());
    }

    #[test]
    fn every_entry_file_is_embedded() -> Result<(), std::io::Error> {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("entries");
        let mut on_disk = Vec::new();
        if dir.is_dir() {
            for file in std::fs::read_dir(dir)? {
                on_disk.push(file?.file_name().to_string_lossy().into_owned());
            }
        }
        on_disk.sort();
        let mut embedded: Vec<_> = BUILTIN.iter().map(|(f, _)| (*f).to_owned()).collect();
        embedded.sort();
        assert_eq!(on_disk, embedded);
        Ok(())
    }

    #[test]
    fn loads_a_valid_entry_and_round_trips_it() -> Result<(), Box<dyn std::error::Error>> {
        let kb = KnowledgeBase::load(&[("cargo-registry.json", CARGO)])?;
        let entry = kb.get("cargo-registry").ok_or("missing")?;
        assert_eq!(entry.safety_class, SafetyClass::SelfManaged);
        let again: Entry = serde_json::from_str(&serde_json::to_string(entry)?)?;
        assert_eq!(&again, entry);
        Ok(())
    }

    #[test]
    fn entries_conform_to_the_published_schema() -> Result<(), Box<dyn std::error::Error>> {
        let schema = serde_json::to_value(schemars::schema_for!(Entry))?;
        let validator = jsonschema::validator_for(&schema)?;
        for (file, json) in BUILTIN.iter().chain([&("cargo-registry.json", CARGO)]) {
            let instance: serde_json::Value = serde_json::from_str(json)?;
            let errors: Vec<String> = validator
                .iter_errors(&instance)
                .map(|e| e.to_string())
                .collect();
            assert_eq!(errors, Vec::<String>::new(), "{file}");
        }
        Ok(())
    }

    #[test]
    fn file_names_ids_and_duplicates_are_checked() {
        assert!(matches!(
            KnowledgeBase::load(&[("cargo.json", CARGO)]),
            Err(LoadError::FileName { .. })
        ));
        assert!(matches!(
            KnowledgeBase::load(&[("cargo-registry.json", CARGO), ("cargo-registry.json", CARGO)]),
            Err(LoadError::Duplicate(id)) if id == "cargo-registry"
        ));
        let unknown_field = CARGO.replacen("\"tool\"", "\"extra\": 1, \"tool\"", 1);
        assert!(matches!(
            KnowledgeBase::load(&[("cargo-registry.json", &unknown_field)]),
            Err(LoadError::Parse { .. })
        ));
    }

    #[test]
    fn date_issues_flag_stale_and_future_verifications() -> Result<(), LoadError> {
        let kb = KnowledgeBase::load(&[("cargo-registry.json", CARGO)])?;
        assert_eq!(kb.date_issues(day("2026-12-01"), 180).len(), 0);
        assert_eq!(
            kb.date_issues(day("2027-10-05"), 180)
                .first()
                .map(|(_, issue)| *issue),
            Some(DateIssue::Stale { days: 365 })
        );
        assert_eq!(
            kb.date_issues(day("2026-10-04"), 180)
                .first()
                .map(|(_, issue)| *issue),
            Some(DateIssue::InFuture)
        );
        Ok(())
    }

    #[test]
    fn every_builtin_entry_has_a_policy_class() {
        let kb = KnowledgeBase::builtin().unwrap_or_else(|e| unreachable!("{e}"));
        let knowledge = kb.knowledge();
        assert_eq!(knowledge.len(), kb.entries().len(), "no entry dropped");
        for entry in kb.entries() {
            let name = SourceName::new(&entry.id).unwrap_or_else(|e| unreachable!("{e}"));
            assert_eq!(knowledge.get(&name), Some(&entry.safety_class.into()));
        }
    }

    #[test]
    fn classes_map_one_to_one() {
        assert_eq!(
            [
                SafetyClass::SelfManaged,
                SafetyClass::ToolAction,
                SafetyClass::Regenerable,
                SafetyClass::NotRegenerable,
            ]
            .map(KnowledgeClass::from),
            [
                KnowledgeClass::SelfManaged,
                KnowledgeClass::ToolAction,
                KnowledgeClass::Regenerable,
                KnowledgeClass::NotRegenerable,
            ]
        );
    }
}
