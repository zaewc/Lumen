import type { CandidateView } from "@lumen/api-client";
import { useMutationState } from "@tanstack/react-query";
import { canChooseReview } from "@/shared/api";

/**
 * The key both plans of this page share, the toolbar's clean-up and one
 * item's move: while either is planning, neither starts another, so a second
 * plan never replaces the one about to be executed.
 */
export const PLANNING = ["plan"] as const;

/**
 * What an item's sheet offers for moving it as the person's own choice
 * (ADR-0051): the button, a line saying it cannot be chosen, or nothing (not a
 * review item, or a clean-up of this scan already ran: scan again to move
 * more).
 */
export type MoveOffer = "move" | "unavailable" | null;

export function moveOffer(candidate: CandidateView, cleanedUp: boolean): MoveOffer {
  if (cleanedUp || candidate.verdict !== "review") {
    return null;
  }
  return canChooseReview(candidate) ? "move" : "unavailable";
}

/** One plan the page made, as the shared mutation cache keeps it. */
export interface PlanState {
  readonly variables: unknown;
  readonly status: "idle" | "pending" | "success" | "error";
  readonly error: unknown;
}

/** The latest plan: whose it was (an item's ID, or `null` for the toolbar's clean-up) and whether making it failed. */
export interface LatestPlan {
  readonly item: string | null;
  /** The scan an item's plan was made for; `null` for the toolbar's. */
  readonly scan: string | null;
  readonly failed: boolean;
  readonly error: unknown;
}

/**
 * The latest of this page's plans (its [`PLANNING`] mutations, oldest first),
 * or `null` before any. Only the action that made it shows its errors and
 * retries it; the item's ID outlives the page in the mutation cache.
 */
export function latestPlan(plans: readonly PlanState[]): LatestPlan | null {
  const last = plans.at(-1);
  if (last === undefined) {
    return null;
  }
  const { variables } = last;
  const fields = typeof variables === "object" && variables !== null ? variables : {};
  const item = "id" in fields && typeof fields.id === "string" ? fields.id : null;
  const scan = "scan" in fields && typeof fields.scan === "string" ? fields.scan : null;
  return { item, scan, failed: last.status === "error", error: last.error };
}

/**
 * A plan made for one item may be executed only while the scan it was made
 * for is still the one shown; otherwise it is dropped and nothing moves.
 */
export function stillCurrent(plannedFor: string | null, shown: string | null): boolean {
  return plannedFor !== null && plannedFor === shown;
}

/** [`latestPlan`] of this page's plans, kept in the shared mutation cache. */
export function useLatestPlan(): LatestPlan | null {
  return latestPlan(
    useMutationState({
      filters: { mutationKey: PLANNING },
      select: (mutation): PlanState => ({
        variables: mutation.state.variables,
        status: mutation.state.status,
        error: mutation.state.error,
      }),
    }),
  );
}
