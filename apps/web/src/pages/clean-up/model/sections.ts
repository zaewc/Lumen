import type { CandidateView, GroupKindView } from "@lumen/api-client";
import { largestFirst, statusOf } from "@/entities/scan-session";
import { sumBytes } from "@/shared/lib";

/**
 * Candidates that belong to one tool or one removed app (or, in other kinds,
 * to one kind).
 */
export interface SourceGroup {
  /**
   * The developer tool, such as "Xcode", or the removed app's identifier;
   * `null` for the rest of a kind.
   */
  readonly source: string | null;
  /** Largest first. */
  readonly items: readonly CandidateView[];
  /** What they would free together, as a decimal string. */
  readonly bytes: string;
  /** Whether `bytes` is exact; otherwise it is a lower bound ("at least"). */
  readonly exact: boolean;
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
  /** Whether `bytes` is exact; otherwise it is a lower bound. */
  readonly exact: boolean;
}

/**
 * The order of the sections: what people can act on most, first. Items with
 * their own command come first, since only the person can free their space.
 */
export const KIND_ORDER = [
  "command",
  "developer",
  "app_cache",
  "removed_app",
  "app_data",
  "other",
] as const satisfies readonly SectionKind[];

/** The kinds whose items are split by their source: a tool, or a removed app. */
export function splitBySource(kind: SectionKind): boolean {
  return kind === "developer" || kind === "removed_app";
}

function groupOf(source: string | null, items: readonly CandidateView[]): SourceGroup {
  const sorted = items.toSorted(largestFirst);
  return {
    source,
    items: sorted,
    bytes: sumBytes(sorted.map((c) => c.reclaimable_bytes)),
    exact: sorted.every((c) => c.reclaimable_exact),
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
 * `KIND_ORDER`, empty kinds left out), developer tools split by tool and a
 * removed app's data by app, each with its count and size. Items with their
 * own command are in their own section, not in their group's. Sizes are
 * added exactly, and a total is exact only when every item's size is.
 */
export function sectionsOf(candidates: readonly CandidateView[]): KindSection[] {
  return KIND_ORDER.flatMap((kind): KindSection[] => {
    const items = candidates.filter((c) => inSection(kind, c));
    if (items.length === 0) {
      return [];
    }
    const sources = splitBySource(kind)
      ? (() => {
          const bySource = new Map<string | null, CandidateView[]>();
          for (const item of items) {
            const source = item.group.source;
            const members = bySource.get(source) ?? [];
            members.push(item);
            bySource.set(source, members);
          }
          return [...bySource]
            .map(([source, members]) => groupOf(source, members))
            .toSorted(bySize);
        })()
      : [groupOf(null, items)];
    return [
      {
        kind,
        sources,
        count: items.length,
        bytes: sumBytes(sources.map((s) => s.bytes)),
        exact: sources.every((s) => s.exact),
      },
    ];
  });
}
