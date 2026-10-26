import type { CandidateView, GroupKindView } from "@lumen/api-client";
import { largestFirst, statusOf } from "@/entities/scan-session";
import { sumBytes } from "@/shared/lib";

/** Candidates that belong to one tool (or, outside developer tools, to one kind). */
export interface SourceGroup {
  /** The developer tool, such as "Xcode"; `null` for the rest of a kind. */
  readonly source: string | null;
  /** Largest first. */
  readonly items: readonly CandidateView[];
  /** What they would free together, as a decimal string. */
  readonly bytes: string;
}

/**
 * What a section holds: the items to clean up with their tool's own command
 * (ADR-0039), or the rest of one kind of group.
 */
export type SectionKind = "command" | GroupKindView;

/** One section of the list: a kind of group, with its tools (if any) and totals. */
export interface KindSection {
  readonly kind: SectionKind;
  /** Per tool for developer tools, largest first; one group otherwise. */
  readonly sources: readonly SourceGroup[];
  readonly count: number;
  readonly bytes: string;
}

/**
 * The order of the sections: what people can act on most, first. Items with
 * their own command come first, since only the person can free their space.
 */
export const KIND_ORDER = [
  "command",
  "developer",
  "app_cache",
  "app_data",
  "other",
] as const satisfies readonly SectionKind[];

function groupOf(source: string | null, items: readonly CandidateView[]): SourceGroup {
  const sorted = items.toSorted(largestFirst);
  return {
    source,
    items: sorted,
    bytes: sumBytes(sorted.map((c) => c.reclaimable_bytes)),
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

/** Whether `candidate` belongs in the section of `kind`. */
function inSection(kind: SectionKind, candidate: CandidateView): boolean {
  const command = statusOf(candidate) === "command";
  return kind === "command" ? command : !command && candidate.group.kind === kind;
}

/**
 * The candidates as the list shows them: one section per kind (in
 * `KIND_ORDER`, empty kinds left out), developer tools split by tool, each
 * with its count and size. Items with their own command are in their own
 * section, not in their group's. Sizes are added exactly.
 */
export function sectionsOf(candidates: readonly CandidateView[]): KindSection[] {
  return KIND_ORDER.flatMap((kind): KindSection[] => {
    const items = candidates.filter((c) => inSection(kind, c));
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
      },
    ];
  });
}
