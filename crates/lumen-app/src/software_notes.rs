//! AI notes on installed software (ADR-0061), cached on the device.
//!
//! While AI review is on, [`explain`] asks the judge about software it has not
//! explained yet ([`lumen_application::software_notes`]), records every request
//! in the ledger, and keeps the answers in `software-notes.json` next to the
//! database. Each piece of software is asked about at most once per origin,
//! identifier and version for a given model and prompt.
//!
//! The cache holds only those identity fields, the model and prompt version,
//! and the note: no path, user name, usage signal or size. It keeps at most
//! [`MAX_CACHED`] entries, dropping the oldest. A cache that is missing or
//! cannot be read is empty: it never keeps the software list from showing.

use std::path::{Path, PathBuf};

use lumen_application::ports::SoftwareExplainer;
use lumen_application::software_notes::{self, SoftwareSubject};
use lumen_domain::{JudgeDescriptor, SoftwareNote, SoftwareOrigin};
use serde::{Deserialize, Serialize};

use crate::SystemClock;
use crate::runtime::block_on;

/// The most notes kept.
pub const MAX_CACHED: usize = 2000;

/// What a note is about: one origin, identifier and version, as the list
/// shows them. A new version is new software.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SoftwareKey {
    /// Its installer or package manager.
    pub origin: SoftwareOrigin,
    /// Its native identifier.
    pub id: String,
    /// Its version, if recorded.
    pub version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    key: SoftwareKey,
    model: String,
    prompt_version: String,
    note: SoftwareNote,
}

impl Entry {
    fn by(&self, key: &SoftwareKey, judge: &JudgeDescriptor) -> bool {
        self.key == *key
            && self.model == judge.model.as_str()
            && self.prompt_version == judge.prompt_version.to_string()
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CacheFile {
    schema: String,
    entries: Vec<Entry>,
}

const SCHEMA: &str = "lumen.software-notes/1";

/// The cached notes, oldest first.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NoteCache {
    entries: Vec<Entry>,
}

fn file(db: &Path) -> PathBuf {
    db.with_file_name("software-notes.json")
}

impl NoteCache {
    /// The notes cached next to `db`; empty when there is no cache or it
    /// cannot be read.
    #[must_use]
    pub fn load(db: &Path) -> Self {
        let entries = std::fs::read(file(db))
            .ok()
            .and_then(|bytes| serde_json::from_slice::<CacheFile>(&bytes).ok())
            .filter(|cache| cache.schema == SCHEMA)
            .map(|cache| cache.entries)
            .unwrap_or_default();
        let mut cache = Self { entries };
        cache.bound();
        cache
    }

    /// Writes the cache next to `db`.
    ///
    /// # Errors
    ///
    /// An I/O error if the file cannot be written.
    pub fn save(&self, db: &Path) -> std::io::Result<()> {
        let text = serde_json::to_vec(&CacheFile {
            schema: SCHEMA.to_owned(),
            entries: self.entries.clone(),
        })
        .map_err(std::io::Error::other)?;
        std::fs::write(file(db), text)
    }

    /// Whether `judge` already explained `key`.
    #[must_use]
    pub fn holds(&self, key: &SoftwareKey, judge: &JudgeDescriptor) -> bool {
        self.entries.iter().any(|e| e.by(key, judge))
    }

    /// The newest note about `key`, by any judge, when it may be shown.
    #[must_use]
    pub fn shown(&self, key: &SoftwareKey) -> Option<SoftwareNote> {
        self.entries
            .iter()
            .rev()
            .find(|e| e.key == *key)
            .map(|e| e.note)
            .filter(SoftwareNote::shown)
    }

    /// Keeps `note` from `judge` about `key`, replacing its earlier answer and
    /// dropping the oldest entries beyond [`MAX_CACHED`].
    pub fn insert(&mut self, key: SoftwareKey, judge: &JudgeDescriptor, note: SoftwareNote) {
        self.entries.retain(|e| !e.by(&key, judge));
        self.entries.push(Entry {
            key,
            model: judge.model.as_str().to_owned(),
            prompt_version: judge.prompt_version.to_string(),
            note,
        });
        self.bound();
    }

    /// How many notes it holds.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether it holds none.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    fn bound(&mut self) {
        let excess = self.entries.len().saturating_sub(MAX_CACHED);
        self.entries.drain(..excess);
    }
}

/// Why explaining could not be recorded.
#[derive(Debug, thiserror::Error)]
pub enum ExplainError {
    /// The ledger could not be written.
    #[error(transparent)]
    Ledger(#[from] lumen_store_sqlite::StoreError),
    /// The cache could not be written.
    #[error("the software notes could not be saved: {0}")]
    Cache(#[from] std::io::Error),
}

/// Asks `explainer` about the `subjects` not yet explained next to `db`,
/// while AI review stays on there, records every request in the ledger, and
/// returns the updated cache. With AI review off, or no descriptor, nothing
/// is asked and the cache is returned as it was.
///
/// # Errors
///
/// An [`ExplainError`] if the ledger or the cache cannot be written.
pub fn explain(
    db: &Path,
    explainer: &dyn SoftwareExplainer,
    subjects: &[SoftwareSubject<SoftwareKey>],
) -> Result<NoteCache, ExplainError> {
    let mut cache = NoteCache::load(db);
    let Some(judge) = explainer.descriptor() else {
        return Ok(cache);
    };
    let still_on = || crate::ai::load(db).enabled;
    if !still_on() {
        return Ok(cache);
    }
    let outcomes = block_on(software_notes::explain(
        explainer,
        &SystemClock,
        subjects,
        &|key, judge| cache.holds(key, judge),
        &still_on,
    ));
    if outcomes.is_empty() {
        return Ok(cache);
    }
    let asked = outcomes.len();
    let mut ledger = lumen_store_sqlite::LedgerStore::new(lumen_store_sqlite::open_ledger(
        &db.with_file_name("ledger.db"),
    )?);
    for item in &outcomes {
        ledger.append_jev_trace(&item.trace)?;
    }
    for item in outcomes {
        if let Some(note) = item.note {
            cache.insert(item.key, &judge, note);
        }
    }
    cache.save(db)?;
    // A count only: the list itself stays out of logs (ADR-0061 rule 6).
    tracing::info!(asked, "software explained");
    Ok(cache)
}

#[cfg(test)]
mod tests {
    use lumen_domain::{JudgmentConfidence, ModelId, RemovalImpact, SoftwarePurpose, SourceName};
    use lumen_testkit::Fixture;

    use super::*;

    fn judge(model: &str) -> JudgeDescriptor {
        JudgeDescriptor {
            provider: SourceName::new("fake").unwrap_or_else(|_| unreachable!()),
            model: ModelId::new(model).unwrap_or_else(|_| unreachable!()),
            model_version: None,
            prompt_version: "jev-software/1".parse().unwrap_or_else(|_| unreachable!()),
        }
    }

    fn key(id: &str, version: &str) -> SoftwareKey {
        SoftwareKey {
            origin: SoftwareOrigin::Homebrew,
            id: id.to_owned(),
            version: Some(version.to_owned()),
        }
    }

    fn note(injection_suspected: bool) -> SoftwareNote {
        SoftwareNote {
            purpose: SoftwarePurpose::RuntimeOrLibrary,
            removal_impact: RemovalImpact::OtherSoftwareDepends,
            confidence: JudgmentConfidence::High,
            injection_suspected,
        }
    }

    #[test]
    fn notes_round_trip_per_version_and_judge() {
        let fx = Fixture::build(&[]).unwrap_or_else(|e| unreachable!("{e}"));
        let db = fx.path("index.db").unwrap_or_else(|e| unreachable!("{e}"));
        assert!(NoteCache::load(&db).is_empty(), "no cache yet");
        let mut cache = NoteCache::default();
        cache.insert(key("node", "22"), &judge("m1"), note(false));
        cache.insert(key("evil", "1"), &judge("m1"), note(true));
        cache.save(&db).unwrap_or_else(|e| unreachable!("{e}"));
        let loaded = NoteCache::load(&db);
        assert_eq!(loaded, cache);
        assert!(loaded.holds(&key("node", "22"), &judge("m1")));
        assert!(
            !loaded.holds(&key("node", "23"), &judge("m1")),
            "new version"
        );
        assert!(!loaded.holds(&key("node", "22"), &judge("m2")), "new model");
        assert_eq!(loaded.shown(&key("node", "22")), Some(note(false)));
        assert!(
            loaded.holds(&key("evil", "1"), &judge("m1")),
            "not asked again"
        );
        assert_eq!(loaded.shown(&key("evil", "1")), None, "never shown");
        let text = std::fs::read_to_string(file(&db)).unwrap_or_else(|e| unreachable!("{e}"));
        assert!(!text.contains("path"), "{text}");
    }

    #[test]
    fn an_unreadable_cache_is_empty() {
        let fx = Fixture::build(&[]).unwrap_or_else(|e| unreachable!("{e}"));
        let db = fx.path("index.db").unwrap_or_else(|e| unreachable!("{e}"));
        for bad in [&b"{not json"[..], br#"{"schema":"other/9","entries":[]}"#] {
            std::fs::write(file(&db), bad).unwrap_or_else(|e| unreachable!("{e}"));
            assert!(NoteCache::load(&db).is_empty());
        }
    }

    #[test]
    fn the_oldest_entries_are_dropped() {
        let mut cache = NoteCache::default();
        for i in 0..=MAX_CACHED {
            cache.insert(key(&format!("p{i}"), "1"), &judge("m1"), note(false));
        }
        assert_eq!(cache.len(), MAX_CACHED);
        assert!(!cache.holds(&key("p0", "1"), &judge("m1")));
        assert!(cache.holds(&key(&format!("p{MAX_CACHED}"), "1"), &judge("m1")));
        cache.insert(key("p1", "1"), &judge("m1"), note(true));
        assert_eq!(cache.len(), MAX_CACHED, "a new answer replaces the old one");
        assert_eq!(cache.shown(&key("p1", "1")), None);
    }

    struct Fake;

    impl SoftwareExplainer for Fake {
        fn descriptor(&self) -> Option<JudgeDescriptor> {
            Some(judge("m1"))
        }

        fn explain_software<'a>(
            &'a self,
            _request: &'a lumen_domain::SoftwareRequest,
        ) -> lumen_application::ports::SoftwareNoteFuture<'a> {
            Box::pin(std::future::ready(Ok(note(false)).into()))
        }
    }

    fn subject(id: &str) -> SoftwareSubject<SoftwareKey> {
        SoftwareSubject {
            key: key(id, "1"),
            request: lumen_domain::SoftwareRequest::new(lumen_domain::SoftwareFacts {
                origin: SoftwareOrigin::Homebrew,
                id,
                name: None,
                publisher: None,
                version: Some("1"),
                protected: false,
                hidden: false,
                services: Vec::new(),
            })
            .unwrap_or_else(|e| unreachable!("{e}")),
            preferred: true,
        }
    }

    #[test]
    fn explained_software_is_traced_and_cached_once() {
        let fx = Fixture::build(&[]).unwrap_or_else(|e| unreachable!("{e}"));
        let db = fx.path("index.db").unwrap_or_else(|e| unreachable!("{e}"));
        let subjects = [subject("node")];
        let off = explain(&db, &Fake, &subjects).unwrap_or_else(|e| unreachable!("{e}"));
        assert!(off.is_empty(), "AI review not chosen: nothing asked");
        crate::ai::set(&db, true, lumen_domain::Timestamp::UNIX_EPOCH)
            .unwrap_or_else(|e| unreachable!("{e}"));
        let cache = explain(&db, &Fake, &subjects).unwrap_or_else(|e| unreachable!("{e}"));
        assert_eq!(cache.shown(&key("node", "1")), Some(note(false)));
        assert_eq!(NoteCache::load(&db), cache);
        let ledger = lumen_store_sqlite::LedgerStore::new(
            lumen_store_sqlite::open_ledger(&db.with_file_name("ledger.db"))
                .unwrap_or_else(|e| unreachable!("{e}")),
        );
        let hash = subjects[0].request.hash();
        let traced = |ledger: &lumen_store_sqlite::LedgerStore| {
            ledger
                .jev_traces_for_request(&hash)
                .unwrap_or_else(|e| unreachable!("{e}"))
                .len()
        };
        assert_eq!(traced(&ledger), 1);
        explain(&db, &Fake, &subjects).unwrap_or_else(|e| unreachable!("{e}"));
        assert_eq!(traced(&ledger), 1, "cached: not asked again");
    }

    #[test]
    fn nothing_is_asked_while_ai_review_is_off() {
        use lumen_application::ports::NoopJudge;
        let fx = Fixture::build(&[]).unwrap_or_else(|e| unreachable!("{e}"));
        let db = fx.path("index.db").unwrap_or_else(|e| unreachable!("{e}"));
        let explained = explain(&db, &NoopJudge, &[]).unwrap_or_else(|e| unreachable!("{e}"));
        assert!(explained.is_empty());
        assert!(!file(&db).exists(), "nothing written");
    }
}
