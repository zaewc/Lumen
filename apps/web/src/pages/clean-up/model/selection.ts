import type { CandidateView } from "@lumen/api-client";
import { statusOf } from "@/entities/scan-session";
import { canChooseReview } from "@/shared/api";
import { sumBytes } from "@/shared/lib";

/**
 * What a person changed from Lumen's proposal (ADR-0069): the proposals they
 * unchecked and the review items they checked, by ID. Everything else is as
 * proposed, so a new assessment starts from the proposal again.
 */
export interface Selection {
  readonly leftOut: ReadonlySet<string>;
  readonly chosen: ReadonlySet<string>;
}

export const PROPOSED: Selection = { leftOut: new Set(), chosen: new Set() };

/** A proposal: what a plan holds unless it is left out. */
function proposed(candidate: CandidateView): boolean {
  return statusOf(candidate) === "quarantine";
}

/** Whether the item has a checkbox: a proposal, or a review item a person may choose (ADR-0051). */
export function checkable(candidate: CandidateView): boolean {
  return proposed(candidate) || canChooseReview(candidate);
}

export function isChecked(selection: Selection, candidate: CandidateView): boolean {
  return proposed(candidate)
    ? !selection.leftOut.has(candidate.id)
    : canChooseReview(candidate) && selection.chosen.has(candidate.id);
}

function flip(set: ReadonlySet<string>, id: string): ReadonlySet<string> {
  const next = new Set(set);
  if (!next.delete(id)) next.add(id);
  return next;
}

/** The selection with `candidate` checked or unchecked; others unchanged. */
export function toggled(selection: Selection, candidate: CandidateView): Selection {
  if (proposed(candidate)) return { ...selection, leftOut: flip(selection.leftOut, candidate.id) };
  if (canChooseReview(candidate))
    return { ...selection, chosen: flip(selection.chosen, candidate.id) };
  return selection;
}

/** What would move, and what to ask the host to plan. */
export interface Selected {
  readonly count: number;
  /** Decimal bytes. */
  readonly bytes: string;
  readonly chosen: readonly string[];
  readonly leftOut: readonly string[];
}

/** The checked items of `candidates`; IDs no longer in them are dropped. */
export function selectedOf(selection: Selection, candidates: readonly CandidateView[]): Selected {
  const checked = candidates.filter((candidate) => isChecked(selection, candidate));
  return {
    count: checked.length,
    bytes: sumBytes(checked.map((candidate) => candidate.reclaimable_bytes)),
    chosen: checked.filter((candidate) => !proposed(candidate)).map((candidate) => candidate.id),
    leftOut: candidates
      .filter((candidate) => proposed(candidate) && selection.leftOut.has(candidate.id))
      .map((candidate) => candidate.id),
  };
}
