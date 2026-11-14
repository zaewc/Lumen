import type { CandidateView, PlanView } from "@lumen/api-client";

/** UI affordance only; the host rechecks the authoritative ADR-0051 rules. */
export function canChooseReview(item: CandidateView): boolean {
  return (
    item.verdict === "review" &&
    item.tool_command === null &&
    !item.reasons.some((reason) =>
      ["classify.devkb-not-regenerable", "classify.devkb-tool-action"].includes(reason.rule),
    )
  );
}

/**
 * A plan for chosen review items holds exactly those items: refuse a host
 * that ignored the choices or added others, rather than confirm a different
 * plan.
 */
export function chosenPlan(plan: PlanView, ids: readonly string[]): PlanView {
  const wanted = new Set(ids);
  if (wanted.size === 0) {
    return plan;
  }
  const planned = new Set(plan.items.map((item) => item.id));
  if (
    plan.items.length !== wanted.size ||
    planned.size !== plan.items.length ||
    [...wanted].some((id) => !planned.has(id)) ||
    plan.items.some((item) => !canChooseReview(item))
  ) {
    throw new Error(
      "The host did not plan the selected review items; assess again or update Lumen.",
    );
  }
  return plan;
}
