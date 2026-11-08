import type { CandidateView, RootView } from "@lumen/api-client";
import { type Status, statusOf } from "@/entities/scan-session";
import { type SectionKind, sectionsOf } from "./sections";
import { type Rect, squarify } from "./squarify";

/**
 * A region of the storage topography (docs/design/visual-language.md): a kind
 * of group, a developer tool inside it, or one item. Areas are what the items
 * would free; the map is not a folder tree, since the interface has no
 * directory sizes.
 */
export type TopoNode =
  | {
      readonly type: "group";
      readonly id: string;
      readonly kind: SectionKind;
      /** The developer tool; `null` for a kind, or a kind's items without one. */
      readonly source: string | null;
      readonly bytes: bigint;
      readonly count: number;
      readonly children: readonly TopoNode[];
    }
  | {
      readonly type: "item";
      readonly id: string;
      readonly candidate: CandidateView;
      readonly status: Status;
      readonly bytes: bigint;
    };

export type TopoGroup = Extract<TopoNode, { type: "group" }>;

/** A region as it is drawn: a node, the items too small to draw, or a coverage gap. */
export type Region =
  | { readonly type: "node"; readonly node: TopoNode }
  | {
      readonly type: "smaller";
      readonly id: string;
      readonly count: number;
      readonly bytes: bigint;
    }
  | {
      readonly type: "unread";
      readonly id: "unread";
      /** Entries the scan could not read or did not reach. */
      readonly entries: bigint;
    };

export interface Placed {
  readonly region: Region;
  readonly rect: Rect;
  /** 0 for the regions of the frame itself, 1 for theirs, and so on. */
  readonly depth: number;
}

export interface LayoutOptions {
  /** Regions below this share of the frame join one "smaller" region per group. */
  readonly minShare?: number;
  /** The fixed share the "Not read" region takes, since its size is unknown. */
  readonly unreadShare?: number;
  /** Room a group keeps for its label above its regions. */
  readonly header?: number;
  /** Room a group keeps around its regions. */
  readonly gap?: number;
}

function itemNode(candidate: CandidateView): TopoNode {
  return {
    type: "item",
    id: `item:${candidate.path}`,
    candidate,
    status: statusOf(candidate),
    bytes: BigInt(candidate.reclaimable_bytes),
  };
}

/**
 * The topography's groups, in the list's order: one per kind, own-command
 * items first; developer tools split by tool when there is more than one.
 */
export function topographyOf(candidates: readonly CandidateView[]): TopoGroup[] {
  return sectionsOf(candidates).map((section) => {
    const children =
      section.sources.length > 1
        ? section.sources.map(
            (group): TopoNode => ({
              type: "group",
              id: `${section.kind}/${group.source ?? ""}`,
              kind: section.kind,
              source: group.source,
              bytes: BigInt(group.bytes),
              count: group.items.length,
              children: group.items.map(itemNode),
            }),
          )
        : (section.sources[0]?.items ?? []).map(itemNode);
    return {
      type: "group",
      id: section.kind,
      kind: section.kind,
      source: null,
      bytes: BigInt(section.bytes),
      count: section.count,
      children,
    };
  });
}

/** Entries the scan could not read or did not reach, over every root. */
export function unreadEntries(roots: readonly RootView[]): bigint {
  const count = (decimal: string) => (/^\d+$/.test(decimal) ? BigInt(decimal) : 0n);
  return roots
    .filter((root) => !root.coverage_complete)
    .reduce(
      (total, root) => total + count(root.denied) + count(root.failed) + count(root.not_scanned),
      0n,
    );
}

/** The groups from the top down to the group `id`, for the breadcrumb; `null` if absent. */
export function trailTo(nodes: readonly TopoNode[], id: string): TopoGroup[] | null {
  for (const node of nodes) {
    if (node.type !== "group") {
      continue;
    }
    if (node.id === id) {
      return [node];
    }
    const below = trailTo(node.children, id);
    if (below !== null) {
      return [node, ...below];
    }
  }
  return null;
}

/** `nodes` as regions: those below `floor` bytes join one "smaller" region, if two or more. */
function regionsOf(nodes: readonly TopoNode[], floor: number, parent: string): Region[] {
  const shown = nodes.filter((node) => node.bytes > 0n);
  const small = shown.filter((node) => Number(node.bytes) < floor);
  if (small.length < 2) {
    return shown.map((node) => ({ type: "node", node }));
  }
  return [
    ...shown
      .filter((node) => Number(node.bytes) >= floor)
      .map((node): Region => ({ type: "node", node })),
    {
      type: "smaller",
      id: `${parent}/smaller`,
      count: small.length,
      bytes: small.reduce((total, node) => total + node.bytes, 0n),
    },
  ];
}

function bytesOf(region: Region): bigint {
  return region.type === "node" ? region.node.bytes : region.type === "smaller" ? region.bytes : 0n;
}

/**
 * Lays out `nodes` (the top level, or a group's children when zoomed in) in
 * `frame`, nesting each group's regions inside it. Items with nothing to free
 * take no area. With unread entries, a "Not read" region takes a fixed share
 * of the frame, never one scaled as if it were measured.
 */
export function layoutTopography(
  nodes: readonly TopoNode[],
  frame: Rect,
  unread: bigint,
  { minShare = 0.005, unreadShare = 0.04, header = 0, gap = 0 }: LayoutOptions = {},
): Placed[] {
  const total = Number(nodes.reduce((sum, node) => sum + node.bytes, 0n));
  const floor = total * minShare;
  const placed: Placed[] = [];

  const lay = (regions: readonly Region[], within: Rect, depth: number, extra: number) => {
    const weights = regions.map((region) =>
      region.type === "unread" ? extra : Number(bytesOf(region)),
    );
    squarify(weights, within).forEach((rect, n) => {
      const region = regions[n];
      if (region === undefined || rect.width <= 0 || rect.height <= 0) {
        return;
      }
      placed.push({ region, rect, depth });
      if (region.type === "node" && region.node.type === "group") {
        const inner: Rect = {
          x: rect.x + gap,
          y: rect.y + header,
          width: rect.width - 2 * gap,
          height: rect.height - header - gap,
        };
        if (inner.width > 0 && inner.height > 0) {
          lay(regionsOf(region.node.children, floor, region.node.id), inner, depth + 1, 0);
        }
      }
    });
  };

  const top = regionsOf(nodes, floor, "");
  if (unread > 0n) {
    // The share of the frame the gap takes: `extra / (total + extra)`.
    const extra = total > 0 ? (total * unreadShare) / (1 - unreadShare) : 1;
    lay([...top, { type: "unread", id: "unread", entries: unread }], frame, 0, extra);
  } else {
    lay(top, frame, 0, 0);
  }
  return placed;
}

/** Every item under `node`, for narrowing the list to a zoomed group. */
export function candidatesIn(node: TopoNode): CandidateView[] {
  return node.type === "item" ? [node.candidate] : node.children.flatMap(candidatesIn);
}

/** The frame's height: shorter on narrow windows. */
export function heightFor(width: number): number {
  return width < 640 ? 200 : 280;
}

/** Whether a region is large enough for its name and size, or for its size alone. */
export function labelFor(width: number, height: number): "full" | "size" | "none" {
  if (width >= 72 && height >= 38) {
    return "full";
  }
  return width >= 64 && height >= 20 ? "size" : "none";
}

export function appIconLabel(source: string | null): string | null {
  const text = source?.trim();
  if (!text) {
    return null;
  }
  const initials = text
    .split(/[^\p{L}\p{N}]+/u)
    .filter(Boolean)
    .flatMap((part) => part.slice(0, 2).split(/(?=[A-Z0-9])/))
    .map((part) => part.replace(/^[^\p{L}\p{N}]+|[^\p{L}\p{N}]+$/gu, ""))
    .filter(Boolean)
    .slice(0, 2)
    .map((part) => part[0]?.toUpperCase() ?? "")
    .join("");
  return initials || null;
}

export function showAppIcon(width: number, height: number): boolean {
  return width >= 100 && height >= 64;
}
