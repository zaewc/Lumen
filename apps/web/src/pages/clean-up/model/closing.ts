import type { CandidateView } from "@lumen/api-client";
import { sumBytes } from "@/shared/lib";

/**
 * What quitting the apps that keep items in use could let Lumen propose on
 * the next scan. The items stay kept: this is only a hint for people.
 */
export interface ClosingSummary {
  /** The apps to quit, sorted and named once; empty when none was named. */
  readonly apps: readonly string[];
  /** What the items would free together, as a decimal string. */
  readonly bytes: string;
  readonly count: number;
}

/**
 * The kept items that quitting their apps would let Lumen propose, added
 * up exactly; `null` when there are none.
 */
export function closingSummary(candidates: readonly CandidateView[]): ClosingSummary | null {
  const items = candidates.filter((c) => c.verdict === "keep" && c.free_when_closed !== null);
  if (items.length === 0) {
    return null;
  }
  const apps = new Set(items.flatMap((c) => c.free_when_closed?.processes ?? []));
  return {
    apps: [...apps].toSorted((a, b) => a.localeCompare(b)),
    bytes: sumBytes(items.map((c) => c.reclaimable_bytes)),
    count: items.length,
  };
}
