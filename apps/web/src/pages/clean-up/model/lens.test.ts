import type { CandidateView, RelationView } from "@lumen/api-client";
import { describe, expect, it } from "vitest";
import { lensMark, lensOf } from "./lens";

function candidate(
  path: string,
  bytes: string,
  group: CandidateView["group"],
  extra: Partial<CandidateView> = {},
): CandidateView {
  return {
    id: `id${path}`,
    path,
    verdict: "review",
    reclaimable_bytes: bytes,
    reasons: [],
    tool_command: null,
    group,
    ai_note: null,
    free_when_closed: null,
    times: { modified: { state: "not_read" }, accessed: { state: "not_read" }, folder: true },
    facts: [],
    relations: { edges: [], omitted: "0" },
    ...extra,
  };
}

const xcode = { kind: "developer", source: "Xcode" } as const;
const slack = { kind: "app_data", source: "com.tinyspeck.slackmacgap" } as const;
const other = { kind: "other", source: null } as const;

/** `from` owned by Slack, at `hops`. */
function ownedBySlack(from: string, hops: number): RelationView {
  return {
    from: { type: "file", id: `id${from}` },
    kind: "owned_by",
    to: { type: "application", app: "com.tinyspeck.slackmacgap" },
    hops,
  };
}

describe("lensOf", () => {
  it("relates items by tool, then kind, with each relation's count and size", () => {
    const derived = candidate("/derived", "4000", xcode);
    const lens = lensOf(derived, [
      derived,
      candidate("/sim", "3000", xcode),
      candidate("/npm", "500", { kind: "developer", source: "npm" }),
      candidate("/installers", "9", other),
    ]);
    expect([...lens.related]).toEqual([
      ["/sim", "tool"],
      ["/npm", "kind"],
    ]);
    expect(lens.lines).toEqual([
      { relation: "tool", tool: "Xcode", count: 1, bytes: "3000" },
      { relation: "kind", kind: "developer", count: 2, bytes: "3500" },
    ]);
  });

  it("relates items kept open by the same apps, naming only the shared ones", () => {
    const open = (processes: string[]) => ({ free_when_closed: { processes } });
    const discord = candidate("/discord", "8", other, open(["Discord", "Spotify"]));
    const lens = lensOf(discord, [
      discord,
      candidate("/spotify", "2", other, open(["Spotify"])),
      candidate("/arc", "1", other, open(["Arc"])),
    ]);
    expect(lens.lines).toEqual([
      { relation: "processes", processes: ["Spotify"], count: 1, bytes: "2" },
    ]);
    expect(lensMark(lens, "/spotify")).toBe("related");
    expect(lensMark(lens, "/arc")).toBe("lowered");
  });

  it("relates items the evidence links, from either side, by ID only", () => {
    const data = candidate("/slack", "100", other, {
      relations: { edges: [ownedBySlack("/slack", 1), ownedBySlack("/http", 2)], omitted: "0" },
    });
    const http = candidate("/http", "5", other);
    const cache = candidate("/cache", "7", other, {
      relations: { edges: [ownedBySlack("/cache", 1), ownedBySlack("/slack", 2)], omitted: "0" },
    });
    // Same name and folder, but nothing recorded links it.
    const lookalike = candidate("/slack-copy", "1", slack);
    const lens = lensOf(data, [data, http, cache, lookalike]);
    expect(lens.lines).toEqual([{ relation: "evidence", count: 2, bytes: "12" }]);
    expect(lensMark(lens, "/slack-copy")).toBe("lowered");
  });

  it("finds nothing related when nothing is recorded, and draws nothing without a lens", () => {
    const alone = candidate("/alone", "1", other);
    const lens = lensOf(alone, [alone, candidate("/b", "1", other)]);
    expect(lens.lines).toEqual([]);
    expect(lensMark(lens, "/alone")).toBe("inspected");
    expect(lensMark(lens, "/b")).toBe("lowered");
    expect(lensMark(null, "/b")).toBe(null);
  });

  it("marks same-kind items as kin, not related", () => {
    const a = candidate("/a", "1", { kind: "app_cache", source: "com.a" });
    const lens = lensOf(a, [a, candidate("/b", "1", { kind: "app_cache", source: "com.b" })]);
    expect(lensMark(lens, "/b")).toBe("kin");
  });
});
