import type { CandidateView } from "@lumen/api-client";
import { STATUSES, statusOf } from "@/entities/scan-session";

/** Which candidates the list shows: all, or those with one status. */
export const FILTERS = ["all", ...STATUSES] as const;

export type Filter = (typeof FILTERS)[number];

/** Whether `candidate` is shown under `filter`. */
export function shows(filter: Filter, candidate: CandidateView): boolean {
  return filter === "all" || statusOf(candidate) === filter;
}

/** How many candidates each filter shows. */
export function filterCounts(candidates: readonly CandidateView[]): {
  readonly [F in Filter]: number;
} {
  const count = (filter: Filter) => candidates.filter((c) => shows(filter, c)).length;
  return {
    all: count("all"),
    quarantine: count("quarantine"),
    command: count("command"),
    review: count("review"),
    keep: count("keep"),
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
