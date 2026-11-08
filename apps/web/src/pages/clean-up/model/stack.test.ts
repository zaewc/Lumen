import type { CandidateView, FactView } from "@lumen/api-client";
import { describe, expect, it } from "vitest";
import { evidenceStack } from "./stack";

const observed = { kind: "observed" } as const;

function fact(f: FactView["fact"], basis: FactView["basis"] = observed): FactView {
  return { fact: f, basis, source: "lumen.scan", observed_at: "2026-10-16T08:00:00Z" };
}

function candidate(extra: Partial<CandidateView> = {}): CandidateView {
  return {
    id: "v:1",
    path: "/Users/ana/Library/Application Support/OldEditor",
    verdict: "review",
    reclaimable_bytes: "1",
    reclaimable_exact: true,
    reasons: [],
    tool_command: null,
    group: { kind: "app_data", source: "com.example.OldEditor" },
    ai_note: null,
    free_when_closed: null,
    times: {
      modified: { state: "recorded", at: "2025-03-18T10:12:44Z" },
      accessed: { state: "may_lag", at: "2025-11-30T21:03:10Z" },
      folder: true,
    },
    facts: [],
    relations: { edges: [], omitted: "0" },
    ...extra,
  };
}

describe("evidenceStack", () => {
  it("shows review uncertainty and future-code fallbacks without repeating tool or AI rules", () => {
    const review_reasons = {
      schema: "review-reasons/1",
      codes: ["unknown_owner", "tool_required", "future"],
    };
    const lines = evidenceStack(
      candidate({ review_reasons, reasons: [{ rule: "default.review", text: "generic" }] }),
      [],
    ).filter((line) => line.type === "review_reason");
    expect(lines.map((line) => line.code)).toEqual(["unknown_owner", "future"]);
    expect(
      evidenceStack(candidate({ verdict: "keep", review_reasons }), []).some(
        (line) => line.type === "review_reason",
      ),
    ).toBe(false);
  });
  it("puts rules first, by source, with their certainty", () => {
    const lines = evidenceStack(
      candidate({
        reasons: [
          { rule: "protect.location.app-support", text: "" },
          { rule: "protect.in-use-unknown", text: "" },
          { rule: "jev.demote", text: "" },
        ],
      }),
      [],
    );
    expect(lines.slice(0, 3).map((l) => [l.source, l.certainty])).toEqual([
      ["location", "resolved"],
      ["rule", "unknown"],
      ["ai", "partial"],
    ]);
  });

  it("says whether the owner is installed only when something answered", () => {
    const owner = fact({ type: "owning_application", app: "com.example.OldEditor" });
    const gone = fact(
      { type: "application_present", app: "com.example.OldEditor", present: false },
      { kind: "absence", searched: ["macos.applications"] },
    );
    const answered = evidenceStack(candidate({ facts: [owner, gone] }), []);
    expect(answered.filter((l) => l.type === "owner")).toEqual([
      expect.objectContaining({
        app: "com.example.OldEditor",
        present: false,
        certainty: "resolved",
      }),
    ]);
    const unanswered = evidenceStack(candidate({ facts: [owner] }), []);
    expect(unanswered.find((l) => l.type === "owner")).toEqual(
      expect.objectContaining({ present: null, certainty: "partial" }),
    );
  });

  it("is partial about use when some processes could not be checked", () => {
    const lines = evidenceStack(
      candidate({
        facts: [
          fact({ type: "not_in_use_by_visible_processes", unseen: "3" }),
          fact({ type: "in_use", in_use: false }),
        ],
      }),
      [],
    );
    expect(lines.filter((l) => l.source === "process").map((l) => l.certainty)).toEqual([
      "partial",
      "resolved",
    ]);
  });

  it("draws times by how far they hold: recorded, may lag, or unknown", () => {
    const certainties = (times: CandidateView["times"]) =>
      evidenceStack(candidate({ times }), [])
        .filter((l) => l.source === "time")
        .map((l) => l.certainty);
    expect(certainties(candidate().times)).toEqual(["resolved", "partial"]);
    expect(
      certainties({
        modified: { state: "not_recorded" },
        accessed: { state: "not_read" },
        folder: false,
      }),
    ).toEqual(["unknown", "unknown"]);
  });

  it("shows usage after use and times, partial about what the platform records", () => {
    const lines = evidenceStack(
      candidate({
        facts: [
          fact({ type: "project_activity", at: "2026-09-01T00:00:00Z" }),
          fact({
            type: "application_last_opened",
            app: "com.example.OldEditor",
            at: "2025-01-02T00:00:00Z",
          }),
          fact({ type: "application_running", app: "com.example.OldEditor", running: false }),
          fact({ type: "in_use", in_use: false }),
        ],
      }),
      [],
    );
    expect(lines.map((l) => [l.type, l.certainty])).toEqual([
      ["app_running", "resolved"],
      ["in_use", "resolved"],
      ["time", "resolved"],
      ["time", "partial"],
      ["app_opened", "partial"],
      ["project_activity", "partial"],
      ["unrelated", "unknown"],
    ]);
  });

  it("ends with relations, then AI review's note on an item to review only", () => {
    const note = { kind: "app_settings", confidence: "medium" } as const;
    const related = [{ relation: "evidence", count: 1, bytes: "5" }] as const;
    const lines = evidenceStack(candidate({ ai_note: note }), related);
    expect(lines.slice(-2).map((l) => [l.type, l.certainty])).toEqual([
      ["related", "resolved"],
      ["note", "partial"],
    ]);
    const kept = evidenceStack(candidate({ ai_note: note, verdict: "keep" }), []);
    expect(kept.some((l) => l.type === "note")).toBe(false);
    expect(kept.at(-1)).toEqual({ type: "unrelated", source: "relation", certainty: "unknown" });
  });
});
