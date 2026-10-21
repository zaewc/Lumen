import type { CandidateView, VerdictView } from "@lumen/api-client";

/** Which candidates the list shows: all, or those with one verdict. */
export const FILTERS = ["all", "quarantine", "review", "keep"] as const;

export type Filter = (typeof FILTERS)[number];

/** Whether `candidate` is shown under `filter`. */
export function shows(filter: Filter, candidate: CandidateView): boolean {
  return filter === "all" || candidate.verdict === filter;
}

/** How many candidates each filter shows. */
export function filterCounts(candidates: readonly CandidateView[]): {
  readonly [F in Filter]: number;
} {
  const verdict = (v: VerdictView) => candidates.filter((c) => c.verdict === v).length;
  return {
    all: candidates.length,
    quarantine: verdict("quarantine"),
    review: verdict("review"),
    keep: verdict("keep"),
  };
}

/** The search parameters of the Clean up page; "all" is the default and left out. */
export interface CleanUpSearch {
  readonly show?: Exclude<Filter, "all">;
}

/**
 * Reads the Clean up page's search parameters from the URL (untrusted): a
 * known filter is kept, anything else means "all".
 */
export function cleanUpSearch(raw: Record<string, unknown>): CleanUpSearch {
  const { show: value } = raw;
  const show = FILTERS.find((filter) => filter === value);
  return show === undefined || show === "all" ? {} : { show };
}
