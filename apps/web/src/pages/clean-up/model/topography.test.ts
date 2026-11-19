import type { CandidateView, RootView } from "@lumen/api-client";
import { describe, expect, it } from "vitest";
import type { Rect } from "./squarify";
import {
  appIconLabel,
  candidatesIn,
  layoutTopography,
  type Placed,
  showAppIcon,
  topographyOf,
  trailTo,
  unreadEntries,
} from "./topography";

function candidate(
  path: string,
  bytes: string,
  group: CandidateView["group"] = { kind: "other", source: null },
  verdict: CandidateView["verdict"] = "quarantine",
): CandidateView {
  return {
    id: path,
    path,
    verdict,
    reclaimable_bytes: bytes,
    reclaimable_exact: true,
    reasons: [],
    tool_command: null,
    group,
    ai_note: null,
    free_when_closed: null,
    times: { modified: { state: "not_read" }, accessed: { state: "not_read" }, folder: true },
    facts: [],
    relations: { edges: [], omitted: "0" },
  };
}

const xcode = { kind: "developer", source: "Xcode" } as const;
const npm = { kind: "developer", source: "npm" } as const;
const cache = { kind: "app_cache", source: "com.example.app" } as const;
const frame: Rect = { x: 0, y: 0, width: 400, height: 200 };

function area(placed: Placed): number {
  return placed.rect.width * placed.rect.height;
}

function idOf(placed: Placed): string {
  return placed.region.type === "node" ? placed.region.node.id : placed.region.id;
}

describe("app icon rules", () => {
  it("derives short glyphs from the application or tool name", () => {
    expect(appIconLabel("Xcode")).toBe("X");
    expect(appIconLabel("Google Chrome")).toBe("GC");
    expect(appIconLabel("com.example.app")).toBe("CE");
    expect(appIconLabel(null)).toBeNull();
  });

  it("keeps icons for larger tiles and hides them for tighter layouts", () => {
    expect(showAppIcon(160, 120)).toBe(true);
    expect(showAppIcon(100, 70)).toBe(true);
    expect(showAppIcon(90, 60)).toBe(false);
  });
});

describe("topographyOf", () => {
  it("groups by owner, largest first, with the owner's name", () => {
    const groups = topographyOf([
      candidate("/a", "300", xcode),
      candidate("/b", "100", npm),
      candidate("/c", "50", cache),
      candidate("/d", "70", xcode),
    ]);
    expect(groups.map((g) => [g.id, g.source, g.bytes, g.count])).toEqual([
      ["tool:Xcode", "Xcode", 370n, 2],
      ["tool:npm", "npm", 100n, 1],
      ["source:com.example.app", "com.example.app", 50n, 1],
    ]);
    expect(groups[0]?.children.map((n) => n.id)).toEqual(["item:/a", "item:/d"]);
  });

  it("names items nothing owns by their kind", () => {
    const [other] = topographyOf([candidate("/k", "1")]);
    expect([other?.id, other?.source, other?.kind]).toEqual(["kind:other", null, "other"]);
  });

  it("marks each item with its status", () => {
    const [other] = topographyOf([candidate("/k", "1", undefined, "keep")]);
    expect(other?.children[0]).toMatchObject({ type: "item", status: "keep" });
  });
});

describe("trailTo", () => {
  const groups = topographyOf([candidate("/a", "3", xcode), candidate("/b", "5", npm)]);

  it("walks from the top to a group", () => {
    expect(trailTo(groups, "tool:npm")?.map((g) => g.id)).toEqual(["tool:npm"]);
  });

  it("finds no group for an item or a missing ID", () => {
    expect(trailTo(groups, "item:/a")).toBeNull();
    expect(trailTo(groups, "app_data")).toBeNull();
  });
});

describe("layoutTopography", () => {
  it("sizes regions by what they free and nests groups inside", () => {
    const groups = topographyOf([
      candidate("/a", "300", xcode),
      candidate("/b", "100", npm),
      candidate("/c", "400", cache),
    ]);
    const placed = layoutTopography(groups, frame, 0n);
    const top = placed.filter((p) => p.depth === 0);
    expect(top.map(idOf)).toEqual(["source:com.example.app", "tool:Xcode", "tool:npm"]);
    expect(top.map(area)).toEqual([
      expect.closeTo(40000, 6),
      expect.closeTo(30000, 6),
      expect.closeTo(10000, 6),
    ]);
    expect(placed.filter((p) => p.depth === 1).map(idOf)).toEqual([
      "item:/c",
      "item:/a",
      "item:/b",
    ]);
  });

  it("keeps room for a group's label and edge", () => {
    const groups = topographyOf([candidate("/a", "1", cache)]);
    const placed = layoutTopography(groups, frame, 0n, { header: 20, gap: 2 });
    expect(placed[1]?.rect).toEqual({ x: 2, y: 20, width: 396, height: 178 });
  });

  it("joins regions below the share into one smaller region per group", () => {
    const groups = topographyOf([
      candidate("/big", "10000", cache),
      candidate("/s1", "10", cache),
      candidate("/s2", "20", cache),
    ]);
    const placed = layoutTopography(groups, frame, 0n);
    const smaller = placed.find((p) => p.region.type === "smaller");
    expect(smaller?.region).toEqual({
      type: "smaller",
      id: "source:com.example.app/smaller",
      count: 2,
      bytes: 30n,
    });
    expect(placed.some((p) => idOf(p) === "item:/s1")).toBe(false);
  });

  it("leaves a single small region as itself", () => {
    const groups = topographyOf([candidate("/big", "10000", cache), candidate("/s1", "10", cache)]);
    const placed = layoutTopography(groups, frame, 0n);
    expect(placed.some((p) => idOf(p) === "item:/s1")).toBe(true);
    expect(placed.some((p) => p.region.type === "smaller")).toBe(false);
  });

  it("gives items with nothing to free no area", () => {
    const groups = topographyOf([candidate("/a", "5", cache), candidate("/zero", "0", cache)]);
    expect(layoutTopography(groups, frame, 0n).some((p) => idOf(p) === "item:/zero")).toBe(false);
  });

  it("gives unread entries a fixed share, never a measured one", () => {
    const groups = topographyOf([candidate("/a", "5", cache)]);
    const placed = layoutTopography(groups, frame, 12n, { unreadShare: 0.1 });
    const unread = placed.find((p) => p.region.type === "unread");
    expect(unread?.region).toEqual({ type: "unread", id: "unread", entries: 12n });
    expect(unread && area(unread)).toBeCloseTo(8000, 6);
  });

  it("fills the frame with the gap when nothing was measured", () => {
    const placed = layoutTopography([], frame, 3n);
    expect(placed).toHaveLength(1);
    expect(placed[0]?.rect).toEqual(frame);
  });

  it("lays out a group's children when zoomed into it", () => {
    const groups = topographyOf([candidate("/a", "3", xcode), candidate("/b", "1", npm)]);
    const npmGroup = trailTo(groups, "tool:npm")?.at(-1);
    const placed = layoutTopography(npmGroup?.children ?? [], frame, 0n);
    expect(placed.map(idOf)).toEqual(["item:/b"]);
    expect(placed[0]?.rect).toEqual(frame);
  });
});

describe("unreadEntries", () => {
  const root = (complete: boolean, denied: string, failed = "0", notScanned = "0") =>
    ({
      coverage_complete: complete,
      denied,
      failed,
      not_scanned: notScanned,
    }) as RootView;

  it("counts what incomplete roots could not read", () => {
    expect(unreadEntries([root(false, "3", "1", "2"), root(true, "9")])).toBe(6n);
    expect(unreadEntries([root(true, "0")])).toBe(0n);
  });
});

describe("candidatesIn", () => {
  it("collects every item under a group", () => {
    const groups = topographyOf([
      candidate("/a", "3", xcode),
      candidate("/b", "1", xcode),
      candidate("/c", "2", cache),
    ]);
    const [tool] = groups;
    expect(tool && candidatesIn(tool).map((c) => c.path)).toEqual(["/a", "/b"]);
  });
});
