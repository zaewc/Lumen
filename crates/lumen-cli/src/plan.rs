//! `lumen plan`: the stored plan as text, ending with the command that
//! executes it (roadmap 10.13c, ADR-0034), with the items the person chose
//! marked (ADR-0051). Planning itself is `lumen_app::plan`.

use std::path::Path;

use lumen_app::Planned;
use lumen_domain::{ByteCount, Chooser};

use crate::report::human;

/// The plan as text, ending with the command that executes it.
pub(crate) fn to_text(planned: &Planned, db: &Path) -> String {
    let mut lines = vec![format!("Plan {}", planned.plan.id())];
    let mut total: u64 = 0;
    let mut chosen = 0_usize;
    for (n, (item, assessed)) in planned.plan.items().iter().zip(&planned.items).enumerate() {
        let bytes = item.reclaim.reclaimable_now.get();
        total = total.saturating_add(bytes);
        lines.push(format!(
            "{:>3}. {:>9}  {}",
            n + 1,
            human(ByteCount::new(bytes)),
            item.path.display()
        ));
        if item.chosen_by == Chooser::Person {
            chosen += 1;
            lines.push(
                "        * chosen by you: Lumen did not propose it, and keeps it in quarantine until you restore it or remove it for good".into(),
            );
        }
        lines.extend(assessed.reasons.iter().map(|r| format!("        - {r}")));
        lines.extend(crate::assess::guidance_line(assessed, "        "));
    }
    lines.push(format!(
        "{} items ({chosen} chosen by you), {}. Nothing has been moved.",
        planned.plan.items().len(),
        human(ByteCount::new(total))
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
