import type { CandidateView } from "@lumen/api-client";
import { describe, expect, it } from "vitest";
import { checkable, isChecked, PROPOSED, selectedOf, toggled } from "./selection";

function candidate(id: string, verdict: CandidateView["verdict"], bytes: string): CandidateView {
  return {
    id,
    path: `/${id}`,
    verdict,
    reclaimable_bytes: bytes,
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

const a = candidate("a", "quarantine", "100");
const b = candidate("b", "quarantine", "20");
const review = candidate("r", "review", "7");
const kept = candidate("k", "keep", "1");
const command: CandidateView = {
  ...candidate("c", "review", "5"),
  tool_command: {
    entry: "e",
    removes_data: false,
    tool: "pnpm",
    program: "pnpm",
    args: ["store", "prune"],
  },
};
const all = [a, b, review, kept, command];

describe("selection", () => {
  it("starts as Lumen proposed: proposals checked, nothing else", () => {
    expect(all.map((c) => isChecked(PROPOSED, c))).toEqual([true, true, false, false, false]);
    expect(selectedOf(PROPOSED, all)).toEqual({ count: 2, bytes: "120", chosen: [], leftOut: [] });
  });

  it("checks only proposals and choosable review items", () => {
    expect(all.map(checkable)).toEqual([true, true, true, false, false]);
    expect(toggled(PROPOSED, kept)).toBe(PROPOSED);
    expect(toggled(PROPOSED, command)).toBe(PROPOSED);
  });

  it("leaves proposals out and adds chosen review items", () => {
    const selection = toggled(toggled(PROPOSED, a), review);
    expect(selectedOf(selection, all)).toEqual({
      count: 2,
      bytes: "27",
      chosen: ["r"],
      leftOut: ["a"],
    });
    expect(selectedOf(toggled(selection, a), all).leftOut, "checked again").toEqual([]);
  });

  it("drops IDs that a new assessment no longer has", () => {
    const selection = toggled(toggled(PROPOSED, a), review);
    expect(selectedOf(selection, [b])).toEqual({ count: 1, bytes: "20", chosen: [], leftOut: [] });
  });
});
