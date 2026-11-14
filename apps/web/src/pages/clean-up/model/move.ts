import type { CandidateView } from "@lumen/api-client";
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

/**
 * A plan made for one item may be executed only while the scan it was made
 * for is still the one shown; otherwise it is dropped and nothing moves.
 */
export function stillCurrent(plannedFor: string | null, shown: string | null): boolean {
  return plannedFor !== null && plannedFor === shown;
}
