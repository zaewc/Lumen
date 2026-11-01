//! A candidate's times as the decision saw them (ADR-0053), each saying how
//! far it can be relied on. Nothing here is scanned, opened or sent to a model.

use lumen_application::assess::Assessed;
use lumen_domain::{EntryKind, Fact, Subject, Timestamp};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// One of an item's times, and how far it can be relied on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum TimeView {
    /// The platform reported it.
    Recorded {
        /// When.
        #[schema(value_type = String, format = DateTime)]
        at: Timestamp,
    },
    /// The platform reported it, but it may skip or defer updating it
    /// (`relatime`, `noatime`, NTFS), so the real time may be later. Never
    /// proof that the item is unused.
    MayLag {
        /// When, as recorded.
        #[schema(value_type = String, format = DateTime)]
        at: Timestamp,
    },
    /// The item's metadata was read, but the platform kept no value: none,
    /// or one at or before the Unix epoch, which filesystems report when
    /// they keep none.
    NotRecorded,
    /// The item's metadata was not read, so the time is unknown.
    NotRead,
}

/// The times the decision saw.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct TimesView {
    /// When its content last changed: `recorded`, `not_recorded` or
    /// `not_read`.
    pub modified: TimeView,
    /// When it was last read: `may_lag`, `not_recorded` or `not_read`.
    pub accessed: TimeView,
    /// The times are a folder entry's own: they change when entries are
    /// added, removed or renamed in it, never when a file inside changes, so
    /// they do not say when its contents were last used.
    pub folder: bool,
}

/// The candidate's modification and access times, from the decision's
/// `LastModified` and `LastAccessed` facts. When several were recorded, the
/// latest observation wins.
pub(super) fn times(assessed: &Assessed) -> TimesView {
    let entry = &assessed.candidate;
    let candidate = Subject::File {
        identity: entry.identity().clone(),
    };
    // A directory's own metadata always comes from its parent's listing; a
    // file's only when it was read (`lumen_application::collect`).
    let read = entry.kind == EntryKind::Directory || entry.access.is_observed();
    let latest = |pick: fn(&Fact) -> Option<Timestamp>| {
        assessed
            .input
            .evidence
            .iter()
            .filter(|record| record.subject == candidate)
            .filter_map(|record| Some((record.provenance.observed_at, pick(&record.fact)?)))
            .max()
            .map(|(_, at)| at)
    };
    let state = |at: Option<Timestamp>, known: fn(Timestamp) -> TimeView| match at {
        Some(at) if at > Timestamp::UNIX_EPOCH => known(at),
        Some(_) => TimeView::NotRecorded,
        None if read => TimeView::NotRecorded,
        None => TimeView::NotRead,
    };
    TimesView {
        modified: state(
            latest(|fact| match fact {
                Fact::LastModified(at) => Some(*at),
                _ => None,
            }),
            |at| TimeView::Recorded { at },
        ),
        accessed: state(
            latest(|fact| match fact {
                Fact::LastAccessed(at) => Some(*at),
                _ => None,
            }),
            |at| TimeView::MayLag { at },
        ),
        folder: entry.kind == EntryKind::Directory,
    }
}

#[cfg(test)]
mod tests {
    use lumen_domain::{AccessState, DenialReason};

    use super::*;
    use crate::views::fixture::{assessed, at, file, record};

    #[test]
    fn times_say_how_far_they_can_be_relied_on() {
        let seen = assessed(
            EntryKind::Directory,
            AccessState::Readable,
            vec![
                record(file(1), Fact::LastModified(at(100)), 10),
                record(file(1), Fact::LastModified(at(300)), 30),
                record(file(1), Fact::LastAccessed(at(200)), 10),
                record(file(2), Fact::LastAccessed(at(900)), 10),
            ],
        );
        assert_eq!(
            times(&seen),
            TimesView {
                modified: TimeView::Recorded { at: at(300) },
                accessed: TimeView::MayLag { at: at(200) },
                folder: true,
            },
            "the latest observation; access times may lag; another file's never"
        );
        let json = serde_json::to_value(times(&seen)).unwrap_or_default();
        assert_eq!(
            json["accessed"],
            serde_json::json!({"state": "may_lag", "at": "1970-01-01T00:03:20Z"})
        );
    }

    #[test]
    fn missing_and_epoch_times_are_never_shown_as_dates() {
        let epoch = assessed(
            EntryKind::Directory,
            AccessState::Denied(DenialReason::Tcc),
            vec![record(
                file(1),
                Fact::LastAccessed(Timestamp::UNIX_EPOCH),
                10,
            )],
        );
        assert_eq!(
            times(&epoch),
            TimesView {
                modified: TimeView::NotRecorded,
                accessed: TimeView::NotRecorded,
                folder: true,
            },
            "a folder's own metadata comes from its parent's listing"
        );
        let unread = assessed(
            EntryKind::File,
            AccessState::Denied(DenialReason::Tcc),
            Vec::new(),
        );
        assert_eq!(
            times(&unread),
            TimesView {
                modified: TimeView::NotRead,
                accessed: TimeView::NotRead,
                folder: false,
            },
            "unknown, not absent (ADR-0008)"
        );
        assert_eq!(
            serde_json::to_value(TimeView::NotRead).unwrap_or_default(),
            serde_json::json!({"state": "not_read"})
        );
    }
}
