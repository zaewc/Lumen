//! Whether AI review is on (ADR-0037): the user's one-time, revocable consent to
//! send metadata about proposed quarantines to the judge (Jeb or Jev).
//!
//! The choice lives next to the database in `ai-review.json`. It starts off; a
//! missing file means off, and so does one that cannot be read, so AI review
//! is never on without a choice on record.

use std::path::{Path, PathBuf};

use lumen_domain::Timestamp;
use serde::{Deserialize, Serialize};

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
