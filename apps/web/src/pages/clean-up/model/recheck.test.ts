import type { CandidateView, ReassessView } from "@lumen/api-client";
import { describe, expect, it } from "vitest";
import { recheckedOf } from "./recheck";

function candidate(extra: Partial<CandidateView> = {}): CandidateView {
  return {
    id: "v:8",
    path: "/Users/ana/Library/Application Support/discord/Cache",
    verdict: "keep",
    reclaimable_bytes: "1",
    reclaimable_exact: true,
    reasons: [],
    tool_command: null,
    group: { kind: "app_cache", source: "com.hnc.Discord" },
    ai_note: null,
    free_when_closed: null,
    times: {
      modified: { state: "recorded", at: "2026-10-16T08:57:40Z" },
      accessed: { state: "may_lag", at: "2026-10-16T08:57:40Z" },
      folder: true,
    },
    facts: [],
    relations: { edges: [], omitted: "0" },
    ...extra,
  };
}

function view(outcome: ReassessView["items"][number]["outcome"]): ReassessView {
  return {
    cancelled: false,
    items: [{ id: "v:8", before: "keep", gaps: ["in_use"], outcome }],
  };
}

describe("recheckedOf", () => {
  it("says whether the verdict changed, and how", () => {
    expect(
      recheckedOf(
        view({
          kind: "decided",
          candidate: candidate({ verdict: "quarantine" }),
          added_facts: "1",
        }),
        "v:8",
        "keep",
      ),
    ).toEqual({ kind: "now", before: "keep", after: "quarantine" });
    expect(
      recheckedOf(
        view({ kind: "decided", candidate: candidate(), added_facts: "1" }),
        "v:8",
        "keep",
      ),
    ).toEqual({ kind: "still", status: "keep" });
  });

  it("never invents a verdict for a changed, settled or missing item", () => {
    expect(recheckedOf(view({ kind: "changed" }), "v:8", "keep")).toEqual({ kind: "changed" });
    expect(recheckedOf(view({ kind: "settled" }), "v:8", "keep")).toEqual({ kind: "settled" });
    expect(recheckedOf(view({ kind: "settled" }), "v:9", "keep")).toEqual({ kind: "missing" });
  });
});
