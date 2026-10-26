import type { CandidateView } from "@lumen/api-client";
import { describe, expect, it } from "vitest";
import { statusOf, summarize } from "./summary";

function candidate(path: string, verdict: CandidateView["verdict"], bytes: string): CandidateView {
  return {
    path,
    verdict,
    reclaimable_bytes: bytes,
    reasons: [],
    tool_command: null,
    group: { kind: "other", source: null },
    ai_note: null,
  };
}

describe("summarize", () => {
  it("counts and totals each status exactly, largest first", () => {
    const summary = summarize([
      candidate("/a", "quarantine", "1000"),
      candidate("/b", "review", "5"),
      candidate("/c", "quarantine", "18446744073709551615"),
      candidate("/d", "quarantine", "1000"),
    ]);
    expect(summary.quarantine.items.map((c) => c.path)).toEqual(["/c", "/a", "/d"]);
    expect(summary.quarantine.bytes).toBe("18446744073709553615");
    expect(summary.review.items).toHaveLength(1);
    expect(summary.review.bytes).toBe("5");
    expect(summary.keep).toEqual({ status: "keep", items: [], bytes: "0" });
  });

  it("counts items to review with their own command apart from review", () => {
    const command = { tool: "pnpm", program: "pnpm", args: ["store", "prune"] };
    const items = [
      { ...candidate("/store", "review", "15000"), tool_command: command },
      candidate("/unknown", "review", "200"),
      { ...candidate("/kept", "keep", "3"), tool_command: command },
      { ...candidate("/cache", "quarantine", "9"), tool_command: command },
    ];
    expect(items.map(statusOf)).toEqual(["command", "review", "keep", "quarantine"]);
    const summary = summarize(items);
    expect(summary.command.items.map((c) => c.path)).toEqual(["/store"]);
    expect(summary.command.bytes).toBe("15000");
    expect(summary.review.items.map((c) => c.path)).toEqual(["/unknown"]);
    expect(summary.quarantine.items.map((c) => c.path)).toEqual(["/cache"]);
  });

  it("refuses a size that is not a decimal count", () => {
    expect(() => summarize([candidate("/a", "keep", "1e3")])).toThrow();
  });
});
