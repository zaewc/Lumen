//! Planning (roadmap 10.13c, 11.3, ADR-0034, ADR-0051): the latest completed
//! scan is assessed again, and every candidate whose verdict is `QUARANTINE`
//! goes into a plan, with the review items the person chose, stored as
//! proposed with its decisions and its hash.

use std::collections::BTreeSet;
use std::path::Path;

use lumen_application::assess::Assessed;
use lumen_application::confirm::NOT_REGENERABLE;
use lumen_application::ports::{Clock, IdGenerator};
use lumen_domain::{Chooser, CleanupPlan, PlannedItem, PolicyDecision, RawPath, Subject, Verdict};
use lumen_store_sqlite::{IndexStore, StoreError, open_index};

use crate::assess::{AssessError, tool_action};

/// The rule that leaves an item for its tool's own command.
const TOOL_ACTION: &str = "classify.devkb-tool-action";

/// Why a path the person chose cannot enter a plan (ADR-0051).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum NotChoosable {
    /// The latest assessment has no candidate at that path.
    Unknown,
    /// The policy keeps it.
    Kept,
    /// The policy already proposes it; plan it without choosing it.
    Proposed,
    /// Only its tool's own command may clean it.
    ToolCommand,
    /// It cannot be regenerated.
    NotRegenerable,
}

impl std::fmt::Display for NotChoosable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Unknown => "the latest assessment has no item there",
            Self::Kept => "Lumen keeps it",
            Self::Proposed => "Lumen already proposes it; plan without choosing it",
            Self::ToolCommand => "only its tool's own command may clean it",
            Self::NotRegenerable => "it cannot be regenerated",
        })
    }
}

/// Why no plan was made.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum PlanError {
    /// The assessment failed.
    #[error(transparent)]
    Assess(#[from] AssessError),
    /// No candidate is proposed for quarantine and none was chosen.
    #[error("nothing to plan: no candidate is proposed for quarantine")]
    NothingToPlan,
    /// A path the person chose cannot be planned; nothing was planned.
    #[error("{path} cannot be chosen: {why}")]
    NotChoosable {
        /// The path, for display.
        path: String,
        /// Why.
        why: NotChoosable,
    },
    /// The plan could not be stored.
    #[error("the index database failed: {0}")]
    Store(#[from] StoreError),
}

/// A stored plan and the assessments behind its items, in plan order.
#[derive(Debug)]
pub struct Planned {
    /// The plan.
    pub plan: CleanupPlan,
    /// Each item's assessment, in the plan's order.
    pub items: Vec<Assessed>,
}

/// Whether a person may choose an item decided as `decision` (ADR-0051):
/// only a `REVIEW` item without a tool command and that can be regenerated
/// or is unknown.
///
/// # Errors
///
/// Why it may not be chosen.
pub fn choosable(decision: &PolicyDecision, has_tool_command: bool) -> Result<(), NotChoosable> {
    match decision.verdict() {
        Verdict::Keep => return Err(NotChoosable::Kept),
        Verdict::Quarantine => return Err(NotChoosable::Proposed),
        Verdict::Review => {}
    }
    let fired = |id: &str| decision.fired_rules().iter().any(|f| f.rule.as_str() == id);
    if fired(NOT_REGENERABLE) {
        Err(NotChoosable::NotRegenerable)
    } else if has_tool_command || fired(TOOL_ACTION) {
        Err(NotChoosable::ToolCommand)
    } else {
        Ok(())
    }
}

/// The assessments that enter the plan, with who chose each: every
/// `QUARANTINE` candidate, and every `chosen` path, each of which must be a
/// choosable review item. Never drops a chosen path silently.
fn pick(
    assessed: Vec<Assessed>,
    chosen: &[RawPath],
    has_tool_command: impl Fn(&Assessed) -> bool,
) -> Result<Vec<(Assessed, Chooser)>, PlanError> {
    let chosen: BTreeSet<&RawPath> = chosen.iter().collect();
    for path in &chosen {
        let refuse = |why| PlanError::NotChoosable {
            path: path.display(),
            why,
        };
        let item = assessed
            .iter()
            .find(|a| &a.candidate.path == *path)
            .ok_or_else(|| refuse(NotChoosable::Unknown))?;
        choosable(&item.decision, has_tool_command(item)).map_err(refuse)?;
    }
    Ok(assessed
        .into_iter()
        .filter_map(|a| match a.decision.verdict() {
            Verdict::Quarantine => Some((a, Chooser::Policy)),
            _ if chosen.contains(&a.candidate.path) => Some((a, Chooser::Person)),
            _ => None,
        })
        .collect())
}

/// Plans every `QUARANTINE` candidate of the latest scan in `db`, and the
/// review items at the `chosen` paths (ADR-0051), and stores the plan as
/// proposed, with its decisions. Other `REVIEW` and all `KEEP` candidates
/// never enter a plan, and review items are not described (ADR-0041): a note
/// could not change a plan.
///
/// # Errors
///
/// A [`PlanError`]: the assessment failed, a chosen path cannot be chosen,
/// nothing is proposed or chosen, or the plan could not be stored.
#[tracing::instrument(name = "plan", skip_all, fields(chosen = chosen.len()))]
pub fn plan(
    db: &Path,
    clock: &dyn Clock,
    ids: &dyn IdGenerator,
    judge: &dyn lumen_application::ports::JudgeModel,
    chosen: &[RawPath],
) -> Result<Planned, PlanError> {
    let assessed = crate::assess::assess(db, clock, judge, &lumen_application::ports::NoopJudge)?;
    let picked = pick(assessed, chosen, |a| tool_action(a).is_some())?;
    if picked.is_empty() {
        return Err(PlanError::NothingToPlan);
    }
    let items: Vec<PlannedItem> = picked
        .iter()
        .map(|(a, chosen_by)| PlannedItem {
            subject: Subject::File {
                identity: a.candidate.identity().clone(),
            },
            path: a.candidate.path.clone(),
            policy_version: a.decision.policy_version(),
            evidence_hash: a.decision.evidence_hash(),
            reclaim: a
                .input
                .observed
                .as_ref()
                .map(|o| o.reclaim)
                .unwrap_or_default(),
            chosen_by: *chosen_by,
        })
        .collect();
    let plan = CleanupPlan::new(ids.plan_id(), clock.now(), items);
    // The plan sorts its items by path; order the assessments the same way.
    let mut ordered = Vec::with_capacity(picked.len());
    for item in plan.items() {
        if let Some((a, _)) = picked.iter().find(|(a, _)| a.candidate.path == item.path) {
            ordered.push(a.clone());
        }
    }
    let decisions: Vec<_> = ordered.iter().map(|a| a.decision.clone()).collect();
    IndexStore::new(open_index(db)?).save_plan(&plan, &decisions)?;
    tracing::info!(items = plan.items().len(), "plan stored");
    Ok(Planned {
        plan,
        items: ordered,
    })
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};
    use std::num::NonZeroU32;

    use lumen_domain::{
        AccessState, ByteCount, EntryKind, EntryTimes, EvidenceHash, FileId, FileIdentity,
        FilesystemEntry, FiredRule, JevEffect, Platform, PolicyStage, Protection, Risk, RiskLevel,
        RuleId, SizeFacts, SizeFlags, VolumeId,
    };
    use lumen_policy::{Context, PolicyInput};

    use super::*;

    fn decision(verdict: Verdict, rule: &str) -> PolicyDecision {
        PolicyDecision::new(
            verdict,
            lumen_policy::POLICY_VERSION,
            EvidenceHash::from_bytes([3; 32]),
            [FiredRule {
                rule: RuleId::new(rule).unwrap_or_else(|e| unreachable!("{e}")),
                stage: PolicyStage::Classification,
                evidence: BTreeSet::new(),
            }],
            Risk {
                level: RiskLevel::Low,
                factors: BTreeSet::new(),
            },
            JevEffect::None,
        )
        .unwrap_or_else(|e| unreachable!("{e}"))
    }

    #[test]
    fn only_review_items_without_a_tool_or_irreplaceable_data_can_be_chosen() {
        let review = decision(Verdict::Review, "default.review");
        assert_eq!(choosable(&review, false), Ok(()));
        assert_eq!(
            choosable(&review, true),
            Err(NotChoosable::ToolCommand),
            "the tool's own command, or nothing"
        );
        let cases = [
            (Verdict::Keep, "default.keep", NotChoosable::Kept),
            (
                Verdict::Quarantine,
                "classify.devkb-regenerable",
                NotChoosable::Proposed,
            ),
            (
                Verdict::Review,
                NOT_REGENERABLE,
                NotChoosable::NotRegenerable,
            ),
            (Verdict::Review, TOOL_ACTION, NotChoosable::ToolCommand),
        ];
        for (verdict, rule, why) in cases {
            assert_eq!(
                choosable(&decision(verdict, rule), false),
                Err(why),
                "{rule}"
            );
        }
    }

    fn raw(text: &str) -> RawPath {
        RawPath::from_unix_bytes(text.as_bytes().to_vec()).unwrap_or_else(|e| unreachable!("{e}"))
    }

    /// A candidate at `path` decided as `decision`.
    fn assessed(path: &str, decision: PolicyDecision) -> Assessed {
        let identity = FileIdentity {
            volume: VolumeId::new("v").unwrap_or_else(|e| unreachable!("{e}")),
            file: FileId::new(u128::from(path.len() as u64)),
        };
        let candidate = FilesystemEntry {
            path: raw(path),
            kind: EntryKind::Directory,
            size: SizeFacts {
                identity: identity.clone(),
                logical: ByteCount::new(4096),
                allocated: ByteCount::new(4096),
                private: None,
                clone_id: None,
                link_count: NonZeroU32::MIN,
                flags: SizeFlags::default(),
            },
            times: EntryTimes::default(),
            protection: Protection::default(),
            access: AccessState::Readable,
        };
        Assessed {
            input: PolicyInput {
                subject: Subject::File { identity },
                evidence_hash: decision.evidence_hash(),
                evidence: Vec::new(),
                paths: vec![candidate.path.clone()],
                context: Context {
                    platform: Platform::Macos,
                    home: None,
                    windows_dir: None,
                    local_app_data: None,
                    mechanisms: BTreeSet::new(),
                    complete_sources: BTreeSet::new(),
                    single_user: false,
                },
                observed: None,
                knowledge: BTreeMap::new(),
                jev: None,
            },
            candidate,
            decision,
            trace: None,
            reasons: Vec::new(),
            note: None,
            note_trace: None,
        }
    }

    #[test]
    fn proposed_items_and_chosen_review_items_are_planned_with_their_chooser() {
        let items = vec![
            assessed(
                "/h/proposed",
                decision(Verdict::Quarantine, "classify.devkb-regenerable"),
            ),
            assessed("/h/chosen", decision(Verdict::Review, "default.review")),
            assessed("/h/left", decision(Verdict::Review, "default.review")),
            assessed("/h/kept", decision(Verdict::Keep, "default.keep")),
        ];
        let chosen = [raw("/h/chosen"), raw("/h/chosen")];
        let picked: Vec<(String, Chooser)> = pick(items.clone(), &chosen, |_| false)
            .unwrap_or_else(|e| unreachable!("{e}"))
            .into_iter()
            .map(|(a, by)| (a.candidate.path.display(), by))
            .collect();
        assert_eq!(
            picked,
            [
                ("/h/proposed".to_owned(), Chooser::Policy),
                ("/h/chosen".to_owned(), Chooser::Person),
            ],
            "nothing is chosen that the person did not choose"
        );
        for (path, why) in [
            ("/h/kept", NotChoosable::Kept),
            ("/h/proposed", NotChoosable::Proposed),
        ] {
            assert!(matches!(
                pick(items.clone(), &[raw("/h/chosen"), raw(path)], |_| false),
                Err(PlanError::NotChoosable { why: w, .. }) if w == why
            ));
        }
        assert!(matches!(
            pick(items, &chosen, |a| a.candidate.path == raw("/h/chosen")),
            Err(PlanError::NotChoosable {
                why: NotChoosable::ToolCommand,
                ..
            })
        ));
    }

    #[test]
    fn an_unknown_chosen_path_fails_the_plan() {
        let error = pick(Vec::new(), &[raw("/home/t/nowhere")], |_| false).err();
        assert!(
            matches!(
                error,
                Some(PlanError::NotChoosable {
                    why: NotChoosable::Unknown,
                    ..
                })
            ),
            "{error:?}"
        );
        assert!(matches!(pick(Vec::new(), &[], |_| false), Ok(v) if v.is_empty()));
    }
}
