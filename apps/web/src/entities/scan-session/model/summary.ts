import type { CandidateView } from "@lumen/api-client";
import { sumBytes } from "@/shared/lib";

/**
 * How an item is presented (ADR-0039): the policy's verdict, except that an
 * item to review that has its tool's own cleanup command is "command": the
 * person's action is to run that command, which Lumen shows and never runs
 * (ADR-0035).
 */
export const STATUSES = ["quarantine", "command", "review", "keep"] as const;

export type Status = (typeof STATUSES)[number];

/** The status `candidate` is presented with. */
export function statusOf(candidate: CandidateView): Status {
  return candidate.verdict === "review" && candidate.tool_command !== null
    ? "command"
    : candidate.verdict;
}

/** The candidates with one status, largest first, and what they would free. */
export interface StatusGroup {
  readonly status: Status;
  readonly items: readonly CandidateView[];
  /** The sum of their reclaimable bytes, as a decimal string. */
  readonly bytes: string;
}

/** An assessment, grouped by status. */
export type CandidateSummary = { readonly [S in Status]: StatusGroup };

/** Orders candidates by what they free, largest first, then by path. */
export function largestFirst(a: CandidateView, b: CandidateView): number {
  const difference = BigInt(b.reclaimable_bytes) - BigInt(a.reclaimable_bytes);
  return difference > 0n ? 1 : difference < 0n ? -1 : a.path.localeCompare(b.path);
}

/**
 * Groups candidates by status, with counts and exact byte totals. The
 * "quarantine" group is exactly what a plan would hold; "review" leaves out
 * the items with their own command.
 */
export function summarize(candidates: readonly CandidateView[]): CandidateSummary {
  const group = (status: Status): StatusGroup => {
    const items = candidates.filter((c) => statusOf(c) === status).toSorted(largestFirst);
    return { status, items, bytes: sumBytes(items.map((c) => c.reclaimable_bytes)) };
  };
  return {
    quarantine: group("quarantine"),
    command: group("command"),
    review: group("review"),
    keep: group("keep"),
  };
}
