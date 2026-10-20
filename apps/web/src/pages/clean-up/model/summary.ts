import type { CandidateView, VerdictView } from "@lumen/api-client";
import { sumBytes } from "@/shared/lib";

/** The candidates with one verdict, largest first, and what they would free. */
export interface VerdictGroup {
  readonly verdict: VerdictView;
  readonly items: readonly CandidateView[];
  /** The sum of their reclaimable bytes, as a decimal string. */
  readonly bytes: string;
}

/** An assessment, grouped by verdict. */
export type CandidateSummary = { readonly [V in VerdictView]: VerdictGroup };

function largestFirst(a: CandidateView, b: CandidateView): number {
  const difference = BigInt(b.reclaimable_bytes) - BigInt(a.reclaimable_bytes);
  return difference > 0n ? 1 : difference < 0n ? -1 : a.path.localeCompare(b.path);
}

/**
 * Groups candidates by verdict, with counts and exact byte totals. The
 * "quarantine" group is exactly what a plan would hold.
 */
export function summarize(candidates: readonly CandidateView[]): CandidateSummary {
  const group = (verdict: VerdictView): VerdictGroup => {
    const items = candidates.filter((c) => c.verdict === verdict).toSorted(largestFirst);
    return { verdict, items, bytes: sumBytes(items.map((c) => c.reclaimable_bytes)) };
  };
  return { quarantine: group("quarantine"), review: group("review"), keep: group("keep") };
}
