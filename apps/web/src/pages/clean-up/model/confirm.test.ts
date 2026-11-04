import type { CandidateView } from "@lumen/api-client";
import { describe, expect, it } from "vitest";
import { planRows } from "./confirm";

function candidate(id: string, path: string, bytes: string): CandidateView {
  return {
    id,
    path,
    verdict: "quarantine",
    reclaimable_bytes: bytes,
    reclaimable_exact: true,
    reasons: [],
    tool_command: null,
    group: { kind: "app_cache", source: null },
    ai_note: null,
    free_when_closed: null,
    times: { modified: { state: "not_read" }, accessed: { state: "not_read" }, folder: true },
    facts: [],
    relations: { edges: [], omitted: "0" },
  };
}

describe("planRows", () => {
  it("lists the items largest first, by exact size, with name and folder", () => {
    const rows = planRows([
      candidate("a", "/Users/ana/Library/Caches/com.figma.Desktop", "540000000"),
      candidate("b", "/Users/ana/Library/Developer/Xcode/DerivedData", "4200000000"),
      // Above 2^53: compared exactly, not as floating point.
      candidate("c", "C:\\Users\\ana\\AppData\\Local\\Temp", "9007199254740993"),
      candidate("d", "/Users/ana/.npm/_cacache", "9007199254740992"),
    ]);
    expect(rows.map((row) => row.id)).toEqual(["c", "d", "b", "a"]);
    expect(rows[2]).toEqual({
      id: "b",
      name: "DerivedData",
      folder: "/Users/ana/Library/Developer/Xcode",
      path: "/Users/ana/Library/Developer/Xcode/DerivedData",
      bytes: "4200000000",
    });
    expect(rows[0]?.name).toBe("Temp");
  });
});
