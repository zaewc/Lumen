//! `lumen plan`: a cleanup plan from the latest assessment (roadmap 10.13c,
//! ADR-0034).
//!
//! The latest completed scan is assessed again, and every candidate whose
//! verdict is `QUARANTINE` goes into a plan. `REVIEW` and `KEEP` items never do. The plan is stored as
//! proposed, with its decisions, and printed with each item's reasons and the
//! full plan hash that `lumen quarantine --confirm` will require.

use std::error::Error;
use std::path::Path;

use lumen_application::assess::Assessed;
use lumen_application::ports::{Clock, IdGenerator};
use lumen_domain::{CleanupPlan, PlannedItem, Subject, Verdict};
use lumen_store_sqlite::{IndexStore, open_index};

/// A stored plan and the assessments behind its items, in plan order.
pub(crate) struct Planned {
    /// The plan.
    pub plan: CleanupPlan,
    /// Each item's assessment, in the plan's order.
    pub items: Vec<Assessed>,
}

/// Plans every `QUARANTINE` candidate of the latest scan in `db`.
pub(crate) fn plan(
    db: &Path,
    clock: &dyn Clock,
    ids: &dyn IdGenerator,
) -> Result<Planned, Box<dyn Error>> {
    let chosen: Vec<Assessed> = crate::assess::assess(db, clock)?
        .into_iter()
        .filter(|a| a.decision.verdict() == Verdict::Quarantine)
        .collect();
    if chosen.is_empty() {
        return Err("nothing to plan: no candidate is proposed for quarantine".into());
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
    Ok(Planned {
        plan,
        items: ordered,
    })
}

/// The plan as text, ending with the command that executes it.
pub(crate) fn to_text(planned: &Planned, db: &Path) -> String {
    let mut lines = vec![format!("Plan {}", planned.plan.id())];
    let mut total: u64 = 0;
    for (n, (item, assessed)) in planned.plan.items().iter().zip(&planned.items).enumerate() {
        let bytes = item.reclaim.reclaimable_now.get();
        total = total.saturating_add(bytes);
        lines.push(format!(
            "{:>3}. {:>12} B  {}",
            n + 1,
            bytes,
            item.path.display()
        ));
        lines.extend(assessed.reasons.iter().map(|r| format!("        - {r}")));
    }
    lines.push(format!(
        "{} items, {total} B. Nothing has been moved.",
        planned.plan.items().len()
    ));
    lines.push(format!("Plan hash: {}", planned.plan.hash()));
    lines.push(format!(
        "To quarantine exactly this plan (reversible), run within 24 hours:\n  lumen quarantine --db {} --plan {} --confirm {}",
        db.display(),
        planned.plan.id(),
        planned.plan.hash()
    ));
    lines.join("\n") + "\n"
}
