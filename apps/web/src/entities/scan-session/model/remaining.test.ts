import type { CandidateView, ItemReportView, QuarantineReportView } from "@lumen/api-client";
import { describe, expect, it } from "vitest";
import { remainingAfter } from "./remaining";

function candidate(path: string): CandidateView {
  return {
    id: path,
    path,
    verdict: "quarantine",
    reclaimable_bytes: "1",
    reclaimable_exact: true,
    reasons: [],
    tool_command: null,
    group: { kind: "other", source: null },
    ai_note: null,
    free_when_closed: null,
    times: { modified: { state: "not_read" }, accessed: { state: "not_read" }, folder: true },
    facts: [],
    relations: { edges: [], omitted: "0" },
  };
}

function report(items: readonly ItemReportView[]): QuarantineReportView {
  return { items: [...items], moved_bytes: "0", plan: "p", resolved: "0", warnings: [] };
}

const moved = (path: string): ItemReportView => ({
  path,
  reclaimable_bytes: "1",
  result: { kind: "quarantined", operation: "o" },
});

describe("remainingAfter", () => {
  it("drops what was moved and what was inside it, keeping the order", () => {
    const left = remainingAfter(
      [
        candidate("/a/cache"),
        candidate("/a/cache/inner"),
        candidate("/a/cache-two"),
        candidate("C:\\b"),
        candidate("C:\\b\\c"),
        candidate("/d"),
      ],
      report([moved("/a/cache"), moved("C:\\b")]),
    );
    expect(left.map((c) => c.path)).toEqual(["/a/cache-two", "/d"]);
  });

  it("keeps what was skipped", () => {
    const candidates = [candidate("/a"), candidate("/b")];
    const left = remainingAfter(
      candidates,
      report([
        moved("/a"),
        { path: "/b", reclaimable_bytes: "1", result: { kind: "skipped", reason: "changed" } },
      ]),
    );
    expect(left.map((c) => c.path)).toEqual(["/b"]);
  });
});
