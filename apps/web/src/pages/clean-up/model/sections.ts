import type { CandidateView, GroupKindView, VerdictView } from "@lumen/api-client";
import { largestFirst } from "@/entities/scan-session";
import { sumBytes } from "@/shared/lib";

/** How many candidates have each verdict. */
export type VerdictCounts = { readonly [V in VerdictView]: number };

/** Candidates that belong to one tool (or, outside developer tools, to one kind). */
export interface SourceGroup {
  /** The developer tool, such as "Xcode"; `null` for the rest of a kind. */
  readonly source: string | null;
  /** Largest first. */
  readonly items: readonly CandidateView[];
  /** What they would free together, as a decimal string. */
  readonly bytes: string;
  readonly verdicts: VerdictCounts;
}

/** One section of the list: a kind of group, with its tools (if any) and totals. */
export interface KindSection {
  readonly kind: GroupKindView;
  /** Per tool for developer tools, largest first; one group otherwise. */
  readonly sources: readonly SourceGroup[];
  readonly count: number;
  readonly bytes: string;
  readonly verdicts: VerdictCounts;
}

/** The order of the sections: what people can act on most, first. */
export const KIND_ORDER = [
  "developer",
  "app_cache",
  "app_data",
  "other",
] as const satisfies readonly GroupKindView[];

function verdictCounts(items: readonly CandidateView[]): VerdictCounts {
  const counts = { quarantine: 0, review: 0, keep: 0 };
  for (const item of items) {
    counts[item.verdict] += 1;
  }
  return counts;
}

function groupOf(source: string | null, items: readonly CandidateView[]): SourceGroup {
  const sorted = items.toSorted(largestFirst);
  return {
    source,
    items: sorted,
    bytes: sumBytes(sorted.map((c) => c.reclaimable_bytes)),
    verdicts: verdictCounts(sorted),
  };
}

/** Larger groups first; a group without a known tool goes last. */
function bySize(a: SourceGroup, b: SourceGroup): number {
  if ((a.source === null) !== (b.source === null)) {
    return a.source === null ? 1 : -1;
  }
  const difference = BigInt(b.bytes) - BigInt(a.bytes);
  return difference > 0n
    ? 1
    : difference < 0n
      ? -1
      : (a.source ?? "").localeCompare(b.source ?? "");
}

/**
 * The candidates as the list shows them: one section per kind of group (in
 * `KIND_ORDER`, empty kinds left out), developer tools split by tool, each
 * with its count, size and verdicts. Sizes are added exactly.
 */
export function sectionsOf(candidates: readonly CandidateView[]): KindSection[] {
  return KIND_ORDER.flatMap((kind): KindSection[] => {
    const items = candidates.filter((c) => c.group.kind === kind);
    if (items.length === 0) {
      return [];
    }
    const sources =
      kind === "developer"
        ? [...Map.groupBy(items, (c) => c.group.source)]
            .map(([source, members]) => groupOf(source, members))
            .toSorted(bySize)
        : [groupOf(null, items)];
    return [
      {
        kind,
        sources,
        count: items.length,
        bytes: sumBytes(sources.map((s) => s.bytes)),
        verdicts: verdictCounts(items),
      },
    ];
  });
}

/** The verdicts present in `counts`, in the policy's order, for a summary line. */
export function presentVerdicts(counts: VerdictCounts): VerdictView[] {
  return (["quarantine", "review", "keep"] as const).filter((verdict) => counts[verdict] > 0);
}
