//! `lumen plan`: the stored plan as text, ending with the command that
//! executes it (roadmap 10.13c, ADR-0034). Planning itself is
//! `lumen_app::plan`.

use std::path::Path;

use lumen_app::Planned;

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
