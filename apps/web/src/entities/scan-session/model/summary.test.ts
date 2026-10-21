import type { CandidateView } from "@lumen/api-client";
import { describe, expect, it } from "vitest";
import { summarize } from "./summary";

function candidate(path: string, verdict: CandidateView["verdict"], bytes: string): CandidateView {
  return {
    path,
    verdict,
    reclaimable_bytes: bytes,
    reasons: [],
    tool_command: null,
    group: { kind: "other", source: null },
  };
}

describe("summarize", () => {
  it("counts and totals each verdict exactly, largest first", () => {
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
    expect(summary.keep).toEqual({ verdict: "keep", items: [], bytes: "0" });
  });

  it("refuses a size that is not a decimal count", () => {
    expect(() => summarize([candidate("/a", "keep", "1e3")])).toThrow();
  });
});
