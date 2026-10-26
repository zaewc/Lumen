//! Whether AI review is on (ADR-0037): the user's one-time, revocable consent to
//! send metadata about proposed quarantines, and about up to 20 of the largest
//! items left for review (ADR-0041), to the judge (Jeb or Jev).
//!
//! The choice lives next to the database in `ai-review.json`. It starts off; a
//! missing file means off, and so does one that cannot be read, so AI review
//! is never on without a choice on record.
//!
//! The judge's API key is the one the user stored in the operating system's
//! secret store (ADR-0036): Jeb's when one is stored, otherwise `OpenRouter`'s.
//! Release builds also carry Lumen's built-in Jeb key (ADR-0042), used only
//! when the user stored none. The CLI and the desktop app both find the key,
//! and say where metadata goes, through here.

use std::path::{Path, PathBuf};

use lumen_application::assess::Assessed;
use lumen_application::ports::{SecretError, SecretName, SecretStore};
use lumen_domain::Timestamp;
pub use lumen_jev::openrouter::ApiKey;
use lumen_jev::openrouter::{JevJudge, SetupError};
use serde::{Deserialize, Serialize};

/// Which service a judge key is for (ADR-0031, ADR-0036). Jeb is the default:
/// the owner's school AI Hub.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Service {
    /// Jeb on the school AI Hub (ADR-0036).
    Jeb,
    /// `TypeSafe`'s Jev through `OpenRouter` (ADR-0031).
    OpenRouter,
}

impl Service {
    /// The services in the order a run looks for their keys.
    pub const PREFERENCE: [Self; 2] = [Self::Jeb, Self::OpenRouter];

    /// Where its key is kept.
    pub const fn secret(self) -> SecretName {
        match self {
            Self::Jeb => SecretName::JEB_API_KEY,
            Self::OpenRouter => SecretName::OPENROUTER_API_KEY,
        }
    }

    /// Its name, for people.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Jeb => "Jeb",
            Self::OpenRouter => "OpenRouter",
        }
    }

    /// What AI review sends with this service's key, and where.
    pub const fn disclosure(self) -> &'static str {
        match self {
            Self::Jeb => {
                "Jeb: for each item proposed for quarantine, and for up to 20 of the largest items left for review, its metadata (path with your home as ~, sizes, ages, evidence facts; never file contents) goes to the school's Jeb server (jeb.https.gsmsv.site) with your key. Its operator may keep requests. Jeb can only make a decision more cautious; for review items it only adds a note on what each likely is."
            }
            Self::OpenRouter => {
                "Jev: for each item proposed for quarantine, and for up to 20 of the largest items left for review, its metadata (path with your home as ~, sizes, ages, evidence facts; never file contents) goes to TypeSafe's Jev through OpenRouter with your key. Jev can only make a decision more cautious; for review items it only adds a note on what each likely is."
            }
        }
    }

    /// A judge on this service using `key`.
    ///
    /// # Errors
    ///
    /// A [`SetupError`] if TLS or the judge's descriptor cannot be set up.
    pub fn judge(self, key: ApiKey) -> Result<JevJudge, SetupError> {
        match self {
            Self::Jeb => JevJudge::jeb(key),
            Self::OpenRouter => JevJudge::new(key),
        }
    }
}

/// Lumen's built-in Jeb key (ADR-0042): compiled in from the build-time
/// variable `LUMEN_JEB_BUILTIN_KEY`, which only tag-triggered release builds
/// set. `None` in every other build, so they behave as before.
///
/// This is the only place that variable is read. The key is plain text in
/// the binary, so anyone with a release can read it; nothing here pretends
/// otherwise. It is never shown, logged or put in an error.
#[must_use]
pub fn builtin_key() -> Option<ApiKey> {
    builtin_key_from(option_env!("LUMEN_JEB_BUILTIN_KEY"))
}

/// The built-in key from the build-time value `raw`: none when it is unset,
/// empty or not an API key. Tests call this with a stand-in instead of
/// compiling one in.
#[must_use]
pub fn builtin_key_from(raw: Option<&str>) -> Option<ApiKey> {
    raw.map(str::trim)
        .filter(|key| !key.is_empty())
        .and_then(ApiKey::new)
}

/// Where the judge's key comes from (ADR-0042).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeySource {
    /// The user's own key, from the operating system's secret store.
    Stored,
    /// Lumen's built-in Jeb key, compiled into release builds.
    Builtin,
}

/// The key a run uses, with its service and where it comes from. Its `Debug`
/// output never shows the key.
#[derive(Debug, Clone)]
pub struct JudgeKey {
    service: Service,
    source: KeySource,
    key: ApiKey,
}

impl JudgeKey {
    /// The service the key is for.
    #[must_use]
    pub const fn service(&self) -> Service {
        self.service
    }

    /// Where the key comes from.
    #[must_use]
    pub const fn source(&self) -> KeySource {
        self.source
    }

    /// What AI review sends with this key, and where.
    #[must_use]
    pub const fn disclosure(&self) -> &'static str {
        disclosure(self.service, self.source)
    }

    /// A judge using this key.
    ///
    /// # Errors
    ///
    /// A [`SetupError`] if TLS or the judge's descriptor cannot be set up.
    pub fn judge(self) -> Result<JevJudge, SetupError> {
        self.service.judge(self.key)
    }
}

/// What AI review sends with a key for `service` from `source`, and where.
#[must_use]
pub const fn disclosure(service: Service, source: KeySource) -> &'static str {
    match (service, source) {
        (Service::Jeb, KeySource::Builtin) => {
            "Jeb: for each item proposed for quarantine, and for up to 20 of the largest items left for review, its metadata (path with your home as ~, sizes, ages, evidence facts; never file contents) goes to the school's Jeb server (jeb.https.gsmsv.site) with Lumen's built-in key. Its operator may keep requests. Jeb can only make a decision more cautious; for review items it only adds a note on what each likely is."
        }
        (service, _) => service.disclosure(),
    }
}

/// Why the stored key could not be used.
#[derive(Debug, thiserror::Error)]
pub enum KeyError {
    /// The secret store could not be read.
    #[error(transparent)]
    Store(#[from] SecretError),
    /// The stored value is not a valid API key.
    #[error("the stored {0} API key is not valid")]
    Invalid(&'static str),
}

/// Why a key was not stored.
#[derive(Debug, thiserror::Error)]
pub enum StoreKeyError {
    /// The text is not an API key. The text itself is never repeated.
    #[error("that is not an API key (1-256 visible ASCII characters); nothing was stored")]
    Invalid,
    /// The secret store could not be written.
    #[error(transparent)]
    Store(#[from] SecretError),
}

/// Stores `key`, trimmed, for `service`, after checking it is an API key.
///
/// # Errors
///
/// A [`StoreKeyError`] if it is not a key or the store cannot be written.
pub fn store_key(
    store: &dyn SecretStore,
    service: Service,
    key: &str,
) -> Result<(), StoreKeyError> {
    let key = key.trim();
    if ApiKey::new(key).is_none() {
        return Err(StoreKeyError::Invalid);
    }
    store.set(
        service.secret(),
        &lumen_application::ports::Secret::new(key),
    )?;
    Ok(())
}

/// The platform's secret store: the macOS keychain or the Windows Credential
/// Manager; `None` where there is none yet.
#[must_use]
pub fn os_store() -> Option<Box<dyn SecretStore>> {
    #[cfg(target_os = "macos")]
    return Some(Box::new(lumen_platform_macos::KeychainStore::new()));
    #[cfg(windows)]
    return Some(Box::new(lumen_platform_windows::CredentialStore::new()));
    #[cfg(not(any(target_os = "macos", windows)))]
    None
}

/// Which service has a key stored (Jeb first), without checking the key.
///
/// # Errors
///
/// A [`SecretError`] if the store cannot be read.
pub fn stored_service(store: &dyn SecretStore) -> Result<Option<Service>, SecretError> {
    for service in Service::PREFERENCE {
        // Existence only: reading the value would make the keychain ask.
        if store.contains(service.secret())? {
            return Ok(Some(service));
        }
    }
    Ok(None)
}

/// The key a run uses: Jeb's when stored, otherwise `OpenRouter`'s.
///
/// # Errors
///
/// A [`KeyError`] if the store cannot be read or the stored key is not valid.
pub fn stored_key(store: &dyn SecretStore) -> Result<Option<(Service, ApiKey)>, KeyError> {
    for service in Service::PREFERENCE {
        if let Some(secret) = store.get(service.secret())? {
            let key = ApiKey::new(secret.expose()).ok_or(KeyError::Invalid(service.label()))?;
            return Ok(Some((service, key)));
        }
    }
    Ok(None)
}

/// Which key a run would use, and where it comes from, without reading the
/// stored value: the user's stored key first, then the built-in one when
/// `builtin` says this build has it (ADR-0042).
///
/// # Errors
///
/// A [`SecretError`] if the store cannot be read.
pub fn key_source(
    store: Option<&dyn SecretStore>,
    builtin: bool,
) -> Result<Option<(Service, KeySource)>, SecretError> {
    if let Some(store) = store
        && let Some(service) = stored_service(store)?
    {
        return Ok(Some((service, KeySource::Stored)));
    }
    Ok(builtin.then_some((Service::Jeb, KeySource::Builtin)))
}

/// The key a run uses: the user's stored key ([`stored_key`]) when there is
/// one, otherwise `builtin`, Lumen's built-in Jeb key (ADR-0042). A stored key
/// that cannot be read or is not valid is an error, never a reason to switch
/// to the built-in key.
///
/// # Errors
///
/// A [`KeyError`] if the store cannot be read or the stored key is not valid.
pub fn judge_key(
    store: Option<&dyn SecretStore>,
    builtin: Option<&ApiKey>,
) -> Result<Option<JudgeKey>, KeyError> {
    if let Some(store) = store
        && let Some((service, key)) = stored_key(store)?
    {
        return Ok(Some(JudgeKey {
            service,
            source: KeySource::Stored,
            key,
        }));
    }
    Ok(builtin.map(|key| JudgeKey {
        service: Service::Jeb,
        source: KeySource::Builtin,
        key: key.clone(),
    }))
}

/// The judge for an assessment next to `db`: one only while AI review is on
/// there and a key is available: a valid stored one, or else `builtin`
/// (ADR-0042). Anything else means no judge, so the policy decides alone; a
/// reason is logged, never the key.
#[must_use]
pub fn active_judge(
    db: &Path,
    store: Option<&dyn SecretStore>,
    builtin: Option<&ApiKey>,
) -> Option<JevJudge> {
    if !load(db).enabled {
        return None;
    }
    let key = match judge_key(store, builtin) {
        Ok(Some(found)) => found,
        Ok(None) => {
            let reason = if store.is_none() {
                "no_secret_store"
            } else {
                "no_key"
            };
            tracing::warn!(reason, "AI review skipped");
            return None;
        }
        Err(e) => {
            tracing::warn!(reason = %e, "AI review skipped");
            return None;
        }
    };
    match key.judge() {
        Ok(judge) => Some(judge),
        Err(e) => {
            tracing::warn!(reason = %e, "AI review skipped");
            None
        }
    }
}

/// Records every judge consultation and description request in `assessed`
/// in the ledger next to `db`.
///
/// # Errors
///
/// A [`lumen_store_sqlite::StoreError`] if the ledger cannot be written.
pub fn record_traces(
    db: &Path,
    assessed: &[Assessed],
) -> Result<(), lumen_store_sqlite::StoreError> {
    let traces: Vec<_> = assessed
        .iter()
        .flat_map(|a| a.trace.iter().chain(&a.note_trace))
        .collect();
    if traces.is_empty() {
        return Ok(());
    }
    let mut ledger = lumen_store_sqlite::LedgerStore::new(lumen_store_sqlite::open_ledger(
        &db.with_file_name("ledger.db"),
    )?);
    for trace in traces {
        ledger.append_jev_trace(trace)?;
    }
    Ok(())
}

/// The user's choice.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AiReview {
    /// Whether assessments consult the judge without being asked each time.
    pub enabled: bool,
    /// When the user turned it on; `None` while it has never been on.
    pub consented_at: Option<Timestamp>,
}

/// Why the choice could not be saved.
#[derive(Debug, thiserror::Error)]
#[error("the AI review setting could not be saved to {path}: {source}")]
pub struct AiSettingError {
    path: PathBuf,
    source: std::io::Error,
}

fn file(db: &Path) -> PathBuf {
    db.with_file_name("ai-review.json")
}

/// The choice recorded next to `db`: off unless the user turned it on.
pub fn load(db: &Path) -> AiReview {
    std::fs::read(file(db))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

/// Turns AI review on, recording when, or off (keeping the record of the
/// last consent).
///
/// # Errors
///
/// [`AiSettingError`] if the file cannot be written.
pub fn set(db: &Path, enabled: bool, now: Timestamp) -> Result<AiReview, AiSettingError> {
    let previous = load(db);
    let review = AiReview {
        enabled,
        consented_at: if enabled {
            Some(now)
        } else {
            previous.consented_at
        },
    };
    let path = file(db);
    let text = serde_json::to_vec_pretty(&review).map_err(|e| AiSettingError {
        path: path.clone(),
        source: std::io::Error::other(e),
    })?;
    std::fs::write(&path, text).map_err(|source| AiSettingError { path, source })?;
    Ok(review)
}

#[cfg(test)]
mod tests {
    use lumen_testkit::{Fixture, Node};

    use super::*;

    #[test]
    fn ai_review_is_off_until_turned_on_and_stays_until_turned_off() {
        let fx = Fixture::build(&[]).unwrap_or_else(|e| unreachable!("{e}"));
        let db = fx.path("index.db").unwrap_or_else(|e| unreachable!("{e}"));
        assert_eq!(load(&db), AiReview::default(), "off by default");
        let at = Timestamp::from_unix(1_800_000_000, 0).unwrap_or_else(|_| unreachable!());
        let on = set(&db, true, at).unwrap_or_else(|e| unreachable!("{e}"));
        assert_eq!(load(&db), on);
        assert_eq!(on.consented_at, Some(at));
        let off = set(&db, false, at).unwrap_or_else(|e| unreachable!("{e}"));
        assert!(!load(&db).enabled);
        assert_eq!(
            off.consented_at,
            Some(at),
            "the last consent stays on record"
        );
    }

    #[test]
    fn jeb_is_preferred_and_keys_are_checked() {
        use lumen_application::ports::Secret;
        let store = lumen_testkit::MemorySecretStore::default();
        assert_eq!(stored_service(&store).ok(), Some(None));
        assert!(matches!(stored_key(&store), Ok(None)));
        store
            .set(SecretName::OPENROUTER_API_KEY, &Secret::new("sk-or-1"))
            .unwrap_or_else(|e| unreachable!("{e}"));
        assert_eq!(stored_service(&store).ok(), Some(Some(Service::OpenRouter)));
        store
            .set(SecretName::JEB_API_KEY, &Secret::new("bad key"))
            .unwrap_or_else(|e| unreachable!("{e}"));
        assert_eq!(stored_service(&store).ok(), Some(Some(Service::Jeb)));
        assert!(matches!(stored_key(&store), Err(KeyError::Invalid("Jeb"))));
        store
            .set(SecretName::JEB_API_KEY, &Secret::new("jeb-key"))
            .unwrap_or_else(|e| unreachable!("{e}"));
        assert!(matches!(stored_key(&store), Ok(Some((Service::Jeb, _)))));
    }

    #[test]
    fn only_api_keys_are_stored() {
        let store = lumen_testkit::MemorySecretStore::default();
        for bad in ["", "  ", "has space", "tab\there"] {
            assert!(matches!(
                store_key(&store, Service::Jeb, bad),
                Err(StoreKeyError::Invalid)
            ));
        }
        assert_eq!(stored_service(&store).ok(), Some(None), "nothing stored");
        store_key(&store, Service::Jeb, " jeb-key\n").unwrap_or_else(|e| unreachable!("{e}"));
        let stored = store
            .get(SecretName::JEB_API_KEY)
            .unwrap_or_else(|e| unreachable!("{e}"));
        assert_eq!(
            stored
                .as_ref()
                .map(lumen_application::ports::Secret::expose),
            Some("jeb-key")
        );
    }

    #[test]
    fn no_judge_while_off_or_without_a_key() {
        use lumen_application::ports::Secret;
        let fx = Fixture::build(&[]).unwrap_or_else(|e| unreachable!("{e}"));
        let db = fx.path("index.db").unwrap_or_else(|e| unreachable!("{e}"));
        let store = lumen_testkit::MemorySecretStore::default();
        store
            .set(SecretName::JEB_API_KEY, &Secret::new("jeb-key"))
            .unwrap_or_else(|e| unreachable!("{e}"));
        let builtin = builtin_key_from(Some(BUILTIN));
        assert!(
            active_judge(&db, Some(&store), builtin.as_ref()).is_none(),
            "off, even with a built-in key"
        );
        let at = Timestamp::from_unix(1_800_000_000, 0).unwrap_or_else(|_| unreachable!());
        set(&db, true, at).unwrap_or_else(|e| unreachable!("{e}"));
        assert!(active_judge(&db, None, None).is_none(), "no store");
        let empty = lumen_testkit::MemorySecretStore::default();
        assert!(active_judge(&db, Some(&empty), None).is_none(), "no key");
        assert!(active_judge(&db, Some(&store), None).is_some());
        assert!(
            active_judge(&db, Some(&empty), builtin.as_ref()).is_some(),
            "the built-in key"
        );
        assert!(
            active_judge(&db, None, builtin.as_ref()).is_some(),
            "the built-in key without a secret store"
        );
    }

    /// A stand-in for the built-in key; the real one is never in the
    /// repository (ADR-0042).
    const BUILTIN: &str = "builtin-stand-in-7f3a";

    #[test]
    fn the_built_in_key_is_absent_unless_compiled_in_and_valid() {
        assert!(builtin_key_from(None).is_none(), "unset");
        assert!(builtin_key_from(Some("")).is_none(), "empty secret");
        assert!(builtin_key_from(Some("  \n")).is_none(), "blank");
        assert!(builtin_key_from(Some("has space")).is_none(), "not a key");
        assert_eq!(
            builtin_key_from(Some(" builtin-stand-in-7f3a\n")),
            ApiKey::new(BUILTIN)
        );
    }

    #[test]
    fn a_stored_key_wins_over_the_built_in_one() {
        use lumen_application::ports::Secret;
        let builtin = builtin_key_from(Some(BUILTIN));
        let store = lumen_testkit::MemorySecretStore::default();
        let source = |store: Option<&dyn SecretStore>, builtin: bool| {
            key_source(store, builtin).unwrap_or_else(|e| unreachable!("{e}"))
        };
        let used = |store: Option<&dyn SecretStore>, builtin: Option<&ApiKey>| {
            judge_key(store, builtin)
                .unwrap_or_else(|e| unreachable!("{e}"))
                .map(|key| (key.service(), key.source()))
        };

        // Nothing stored: the built-in key when this build has one.
        assert_eq!(source(Some(&store), false), None);
        assert_eq!(used(Some(&store), None), None);
        assert_eq!(
            source(Some(&store), true),
            Some((Service::Jeb, KeySource::Builtin))
        );
        assert_eq!(
            used(Some(&store), builtin.as_ref()),
            Some((Service::Jeb, KeySource::Builtin))
        );
        assert_eq!(
            used(None, builtin.as_ref()),
            Some((Service::Jeb, KeySource::Builtin))
        );

        // A stored key wins, whichever service it is for.
        store
            .set(SecretName::OPENROUTER_API_KEY, &Secret::new("sk-or-1"))
            .unwrap_or_else(|e| unreachable!("{e}"));
        assert_eq!(
            source(Some(&store), true),
            Some((Service::OpenRouter, KeySource::Stored))
        );
        assert_eq!(
            used(Some(&store), builtin.as_ref()),
            Some((Service::OpenRouter, KeySource::Stored))
        );
        store
            .set(SecretName::JEB_API_KEY, &Secret::new("jeb-key"))
            .unwrap_or_else(|e| unreachable!("{e}"));
        assert_eq!(
            used(Some(&store), builtin.as_ref()),
            Some((Service::Jeb, KeySource::Stored))
        );

        // A broken stored key is an error, not a switch to the built-in key.
        store
            .set(SecretName::JEB_API_KEY, &Secret::new("bad key"))
            .unwrap_or_else(|e| unreachable!("{e}"));
        assert!(matches!(
            judge_key(Some(&store), builtin.as_ref()),
            Err(KeyError::Invalid("Jeb"))
        ));
    }

    #[test]
    fn the_disclosure_names_whose_key_is_used() {
        let builtin = builtin_key_from(Some(BUILTIN));
        let key = judge_key(None, builtin.as_ref())
            .unwrap_or_else(|e| unreachable!("{e}"))
            .unwrap_or_else(|| unreachable!());
        assert!(key.disclosure().contains("Lumen's built-in key"));
        assert!(key.disclosure().contains("jeb.https.gsmsv.site"));
        assert_eq!(
            key.disclosure(),
            disclosure(Service::Jeb, KeySource::Stored)
                .replace("with your key", "with Lumen's built-in key"),
            "the same disclosure, but for whose key"
        );
    }

    /// Every field of every span and event, as text.
    #[derive(Clone, Default)]
    struct Captured(std::sync::Arc<std::sync::Mutex<String>>);

    impl Captured {
        fn text(&self) -> String {
            self.0
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .clone()
        }
    }

    impl tracing::field::Visit for Captured {
        fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
            let mut text = self
                .0
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            std::fmt::Write::write_fmt(&mut *text, format_args!("{}={value:?}\n", field.name()))
                .unwrap_or_else(|_| unreachable!("writing to a String cannot fail"));
        }
    }

    impl tracing::Subscriber for Captured {
        fn enabled(&self, _: &tracing::Metadata<'_>) -> bool {
            true
        }
        fn new_span(&self, span: &tracing::span::Attributes<'_>) -> tracing::span::Id {
            span.record(&mut self.clone());
            tracing::span::Id::from_u64(1)
        }
        fn record(&self, _: &tracing::span::Id, values: &tracing::span::Record<'_>) {
            values.record(&mut self.clone());
        }
        fn record_follows_from(&self, _: &tracing::span::Id, _: &tracing::span::Id) {}
        fn event(&self, event: &tracing::Event<'_>) {
            event.record(&mut self.clone());
        }
        fn enter(&self, _: &tracing::span::Id) {}
        fn exit(&self, _: &tracing::span::Id) {}
    }

    #[test]
    fn the_built_in_key_is_never_shown_logged_or_traced() {
        use lumen_application::ports::{JudgeModel as _, Secret};
        let fx = Fixture::build(&[]).unwrap_or_else(|e| unreachable!("{e}"));
        let db = fx.path("index.db").unwrap_or_else(|e| unreachable!("{e}"));
        let at = Timestamp::from_unix(1_800_000_000, 0).unwrap_or_else(|_| unreachable!());
        set(&db, true, at).unwrap_or_else(|e| unreachable!("{e}"));
        let builtin = builtin_key_from(Some(BUILTIN));
        let broken = lumen_testkit::MemorySecretStore::default();
        broken
            .set(SecretName::JEB_API_KEY, &Secret::new("bad key"))
            .unwrap_or_else(|e| unreachable!("{e}"));

        let logs = Captured::default();
        let (judge, refused) = tracing::subscriber::with_default(logs.clone(), || {
            let judge = active_judge(&db, None, builtin.as_ref());
            let refused = active_judge(&db, Some(&broken), builtin.as_ref());
            (judge, refused)
        });
        assert!(refused.is_none());
        assert!(logs.text().contains("AI review skipped"), "{}", logs.text());
        let judge = judge.unwrap_or_else(|| unreachable!());

        let key = judge_key(None, builtin.as_ref())
            .unwrap_or_else(|e| unreachable!("{e}"))
            .unwrap_or_else(|| unreachable!());
        let error = judge_key(Some(&broken), builtin.as_ref())
            .err()
            .unwrap_or_else(|| unreachable!());
        // Traces record the judge's descriptor, never its key.
        let descriptor = judge.descriptor().unwrap_or_else(|| unreachable!());
        for shown in [
            logs.text(),
            format!("{key:?}"),
            format!("{judge:?}"),
            format!("{error} {error:?}"),
            format!("{descriptor:?}"),
            serde_json::to_string(&descriptor).unwrap_or_default(),
        ] {
            assert!(!shown.contains(BUILTIN), "{shown}");
        }
    }

    #[test]
    fn an_unreadable_setting_means_off() {
        let fx = Fixture::build(&[Node::File {
            path: "ai-review.json".into(),
            contents: b"{\"enabled\": true, \"extra\": 1}".to_vec(),
        }])
        .unwrap_or_else(|e| unreachable!("{e}"));
        let db = fx.path("index.db").unwrap_or_else(|e| unreachable!("{e}"));
        assert!(!load(&db).enabled);
    }
}
