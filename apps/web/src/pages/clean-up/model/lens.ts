import type { CandidateView, GroupKindView } from "@lumen/api-client";
import { sumBytes } from "@/shared/lib";

/**
 * How another item relates to the inspected one, closest first. Each comes
 * from data the interface has (docs/design/visual-language.md, evidence
 * lens); none is inferred from paths or names.
 */
export type Relation = "tool" | "processes" | "evidence" | "kind";

/** One relation in words: how many other items share it, and their size. */
export type LensLine =
  | {
      readonly relation: "tool";
      readonly tool: string;
      readonly count: number;
      readonly bytes: string;
    }
  | {
      readonly relation: "processes";
      readonly processes: readonly string[];
      readonly count: number;
      readonly bytes: string;
    }
  | { readonly relation: "evidence"; readonly count: number; readonly bytes: string }
  | {
      readonly relation: "kind";
      readonly kind: GroupKindView;
      readonly count: number;
      readonly bytes: string;
    };

export interface Lens {
  /** The inspected item's path. */
  readonly inspected: string;
  /** Every related item's path, with its closest relation. */
  readonly related: ReadonlyMap<string, Relation>;
  /** One line per relation that has items, closest first. */
  readonly lines: readonly LensLine[];
}

/** How an item is drawn while another is inspected. */
export type LensMark = "inspected" | "related" | "kin" | "lowered";

/** The tool or app an item comes from, if known. */
function toolOf(candidate: CandidateView): string | null {
  return candidate.group.source ?? candidate.tool_command?.tool ?? null;
}

/** File IDs at either end of the evidence graph's edges about `candidate`. */
function linkedIds(candidate: CandidateView): Set<string> {
  const ids = new Set<string>();
  for (const edge of candidate.relations.edges) {
    for (const node of [edge.from, edge.to]) {
      if (node.type === "file") {
        ids.add(node.id);
      }
    }
  }
  ids.delete(candidate.id);
  return ids;
}

/**
 * The items related to `inspected`: from the same tool or app, kept open by
 * the same apps, linked by a recorded relation (either way round), or of the
 * same kind. "Other" is not a kind anything shares.
 */
export function lensOf(inspected: CandidateView, candidates: readonly CandidateView[]): Lens {
  const tool = toolOf(inspected);
  const processes = new Set(inspected.free_when_closed?.processes ?? []);
  const linked = linkedIds(inspected);
  const kind = inspected.group.kind === "other" ? null : inspected.group.kind;

  const by: { [R in Relation]: CandidateView[] } = {
    tool: [],
    processes: [],
    evidence: [],
    kind: [],
  };
  const shared = new Set<string>();
  const related = new Map<string, Relation>();
  for (const candidate of candidates) {
    if (candidate.path === inspected.path) {
      continue;
    }
    const sharing = (candidate.free_when_closed?.processes ?? []).filter((p) => processes.has(p));
    const relations: Relation[] = [];
    if (tool !== null && toolOf(candidate) === tool) relations.push("tool");
    if (sharing.length > 0) relations.push("processes");
    if (linked.has(candidate.id) || linkedIds(candidate).has(inspected.id))
      relations.push("evidence");
    if (kind !== null && candidate.group.kind === kind) relations.push("kind");
    for (const relation of relations) {
      by[relation].push(candidate);
    }
    for (const process of sharing) {
      shared.add(process);
    }
    const closest = relations[0];
    if (closest !== undefined) {
      related.set(candidate.path, closest);
    }
  }

  const total = (items: readonly CandidateView[]) => ({
    count: items.length,
    bytes: sumBytes(items.map((c) => c.reclaimable_bytes)),
  });
  const lines: LensLine[] = [];
  if (tool !== null && by.tool.length > 0)
    lines.push({ relation: "tool", tool, ...total(by.tool) });
  if (by.processes.length > 0) {
    lines.push({ relation: "processes", processes: [...shared].sort(), ...total(by.processes) });
  }
  if (by.evidence.length > 0) lines.push({ relation: "evidence", ...total(by.evidence) });
  if (kind !== null && by.kind.length > 0)
    lines.push({ relation: "kind", kind, ...total(by.kind) });
  return { inspected: inspected.path, related, lines };
}

/**
 * How to draw `path` under `lens`: the inspected item, a close relation
 * (outlined), the same kind only (as it is), or anything else (lowered).
 * Without a lens nothing changes.
 */
export function lensMark(lens: Lens | null, path: string): LensMark | null {
  if (lens === null) {
    return null;
  }
  if (path === lens.inspected) {
    return "inspected";
  }
  const relation = lens.related.get(path);
  if (relation === undefined) {
    return "lowered";
  }
  return relation === "kind" ? "kin" : "related";
}
