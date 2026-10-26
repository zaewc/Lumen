//! Planning (roadmap 10.13c, 11.3, ADR-0034): the latest completed scan is
//! assessed again, and every candidate whose verdict is `QUARANTINE` goes
//! into a plan, stored as proposed with its decisions and its hash.

use std::path::Path;

use lumen_application::assess::Assessed;
use lumen_application::ports::{Clock, IdGenerator};
use lumen_domain::{CleanupPlan, PlannedItem, Subject, Verdict};
use lumen_store_sqlite::{IndexStore, StoreError, open_index};

use crate::assess::AssessError;

/// Why no plan was made.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum PlanError {
    /// The assessment failed.
    #[error(transparent)]
    Assess(#[from] AssessError),
    /// No candidate is proposed for quarantine.
    #[error("nothing to plan: no candidate is proposed for quarantine")]
    NothingToPlan,
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

/// Plans every `QUARANTINE` candidate of the latest scan in `db` and stores
/// the plan as proposed, with its decisions. `REVIEW` and `KEEP` candidates
/// never enter a plan, and review items are not described (ADR-0041): a note
/// could not change a plan.
///
/// # Errors
///
/// A [`PlanError`]: the assessment failed, nothing is proposed, or the plan
/// could not be stored.
#[tracing::instrument(name = "plan", skip_all)]
pub fn plan(
    db: &Path,
    clock: &dyn Clock,
    ids: &dyn IdGenerator,
    judge: &dyn lumen_application::ports::JudgeModel,
) -> Result<Planned, PlanError> {
    let chosen: Vec<Assessed> =
        crate::assess::assess(db, clock, judge, &lumen_application::ports::NoopJudge)?
            .into_iter()
            .filter(|a| a.decision.verdict() == Verdict::Quarantine)
            .collect();
    if chosen.is_empty() {
        return Err(PlanError::NothingToPlan);
    }
    let items: Vec<PlannedItem> = chosen
        .iter()
        .map(|a| PlannedItem {
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
        })
        .collect();
    let plan = CleanupPlan::new(ids.plan_id(), clock.now(), items);
    // The plan sorts its items by path; order the assessments the same way.
    let mut ordered = Vec::with_capacity(chosen.len());
    for item in plan.items() {
        if let Some(a) = chosen.iter().find(|a| a.candidate.path == item.path) {
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
