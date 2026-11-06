//! Items checked again with fresh read-only evidence (ADR-0058): what each
//! was, what was unknown about it, and what it is now.

use lumen_application::assess::Assessed;
use lumen_application::reassess::{Gap, Outcome, Reassessed, Reassessment};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use super::{CandidateView, VerdictView, relations};

/// The opaque ID the interface knows `assessed` by ([`CandidateView::id`]).
#[must_use]
pub fn candidate_id(assessed: &Assessed) -> String {
    relations::file_id(assessed.candidate.identity())
}

/// Which items to check again.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ReassessRequest {
    /// Items of the latest assessment, by [`CandidateView::id`]. Empty: every
    /// item kept or reviewed because of something Lumen did not know.
    pub ids: Vec<String>,
}

/// Something the earlier assessment did not know.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ReassessGapView {
    /// Whether a process uses it.
    InUse,
    /// What is inside it.
    Unread,
    /// How it is now.
    Stale,
    /// Who owns it and whether that application is installed.
    Owner,
}

impl From<Gap> for ReassessGapView {
    fn from(gap: Gap) -> Self {
        match gap {
            Gap::InUse => Self::InUse,
            Gap::Unread => Self::Unread,
            Gap::Stale => Self::Stale,
            Gap::Owner => Self::Owner,
        }
    }
}

/// How checking one item again ended.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ReassessOutcomeView {
    /// Its verdict does not rest on anything unknown; nothing was checked.
    Settled,
    /// It changed since the scan, so it was not decided again: scan again.
    Changed,
    /// Decided again from fresh evidence.
    Decided {
        /// The item as decided now.
        candidate: Box<CandidateView>,
        /// How many facts the checks added.
        added_facts: String,
    },
}

/// One item checked again.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ReassessedItemView {
    /// Its ID, as in the assessment.
    pub id: String,
    /// Its earlier verdict.
    pub before: VerdictView,
    /// What was unknown.
    pub gaps: Vec<ReassessGapView>,
    /// How it ended.
    pub outcome: ReassessOutcomeView,
}

/// Items checked again, in order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct ReassessView {
    /// Every item checked before the run ended.
    pub items: Vec<ReassessedItemView>,
    /// The run stopped early; later items were not checked.
    pub cancelled: bool,
}

impl ReassessedItemView {
    /// The view of `item`, which reassessed `earlier`.
    #[must_use]
    pub fn new(earlier: &Assessed, item: &Reassessed) -> Self {
        Self {
            id: candidate_id(earlier),
            before: item.before.verdict.into(),
            gaps: item.gaps.iter().map(|gap| (*gap).into()).collect(),
            outcome: match &item.outcome {
                Outcome::Settled => ReassessOutcomeView::Settled,
                Outcome::Changed => ReassessOutcomeView::Changed,
                Outcome::Decided { after, added } => ReassessOutcomeView::Decided {
                    candidate: Box::new(CandidateView::from(after.as_ref())),
                    added_facts: added.len().to_string(),
                },
            },
        }
    }
}

impl ReassessView {
    /// The view of `reassessment`, whose items reassessed `before`, in order.
    #[must_use]
    pub fn new(before: &[Assessed], reassessment: &Reassessment) -> Self {
        Self {
            items: before
                .iter()
                .zip(&reassessment.items)
                .map(|(earlier, item)| ReassessedItemView::new(earlier, item))
                .collect(),
            cancelled: reassessment.cancelled,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use lumen_application::reassess::Verdicted;
    use lumen_domain::{AccessState, EntryKind, Verdict};

    use super::super::fixture::assessed;
    use super::*;

    #[test]
    fn items_keep_their_id_and_say_how_they_ended() {
        let earlier = assessed(EntryKind::Directory, AccessState::Readable, Vec::new());
        let before = Verdicted {
            verdict: Verdict::Review,
            rules: Vec::new(),
        };
        let reassessment = Reassessment {
            items: vec![
                Reassessed {
                    before: before.clone(),
                    gaps: BTreeSet::new(),
                    outcome: Outcome::Settled,
                },
                Reassessed {
                    before: before.clone(),
                    gaps: BTreeSet::from([Gap::Owner]),
                    outcome: Outcome::Changed,
                },
                Reassessed {
                    before,
                    gaps: BTreeSet::from([Gap::InUse, Gap::Unread]),
                    outcome: Outcome::Decided {
                        after: Box::new(earlier.clone()),
                        added: Vec::new(),
                    },
                },
            ],
            cancelled: true,
        };
        let earlier_three = vec![earlier.clone(), earlier.clone(), earlier.clone()];
        let view = ReassessView::new(&earlier_three, &reassessment);
        assert!(view.cancelled);
        assert!(
            view.items
                .iter()
                .all(|item| item.id == candidate_id(&earlier))
        );
        assert_eq!(view.items[0].outcome, ReassessOutcomeView::Settled);
        assert_eq!(view.items[1].gaps, [ReassessGapView::Owner]);
        assert_eq!(view.items[1].outcome, ReassessOutcomeView::Changed);
        assert_eq!(
            view.items[2].gaps,
            [ReassessGapView::InUse, ReassessGapView::Unread]
        );
        let ReassessOutcomeView::Decided {
            candidate,
            added_facts,
        } = &view.items[2].outcome
        else {
            unreachable!("decided");
        };
        assert_eq!(candidate.id, candidate_id(&earlier));
        assert_eq!(
            candidate.rechecks,
            Some(vec![ReassessGapView::Owner]),
            "an unrecognised item can be checked for its owner"
        );
        assert_eq!(added_facts, "0");
        assert_eq!(view.items[2].before, VerdictView::Review);
    }
}
