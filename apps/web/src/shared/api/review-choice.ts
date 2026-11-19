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

/**
 * A plan of the proposals minus `leftOut`, with the chosen review items
 * (ADR-0069): it holds every chosen ID once, no left-out ID, and otherwise
 * only proposals. Refuse anything else rather than confirm a different plan.
 */
export function selectedPlan(
  plan: PlanView,
  chosen: readonly string[],
  leftOut: readonly string[],
): PlanView {
  const wanted = new Set(chosen);
  const out = new Set(leftOut);
  const ids = plan.items.map((item) => item.id);
  if (
    new Set(ids).size !== ids.length ||
    [...wanted].some((id) => !ids.includes(id)) ||
    plan.items.some(
      (item) =>
        out.has(item.id) ||
        (wanted.has(item.id) ? !canChooseReview(item) : item.verdict !== "quarantine"),
    )
  ) {
    throw new Error("The host did not plan the selected items; assess again or update Lumen.");
  }
  return plan;
}
