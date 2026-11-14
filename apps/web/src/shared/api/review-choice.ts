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

/** Refuse older hosts that ignore choices, rather than confirming a different plan. */
export function chosenPlan(plan: PlanView, ids: readonly string[]): PlanView {
  if (ids.some((id) => !plan.items.some((item) => item.id === id && canChooseReview(item)))) {
    throw new Error(
      "The host did not plan the selected review items; assess again or update Lumen.",
    );
  }
  return plan;
}
