import type { CandidateView, QuarantineReportView, RootView } from "@lumen/api-client";
import { describe, expect, it } from "vitest";
import { closingSummary } from "./closing";
import { coverageOf } from "./coverage";
import { cleanUpSearch, filterCounts, shows } from "./filter";
import { reportSummary } from "./report";
import { cleanUpStep } from "./step";

function candidate(
  path: string,
  verdict: CandidateView["verdict"],
  bytes: string,
  group: CandidateView["group"] = { kind: "other", source: null },
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

const simctl = {
  entry: "e",
  removes_data: false,
  tool: "Xcode",
  program: "xcrun",
  args: ["simctl", "delete", "unavailable"],
};

/** An item to review that has its tool's own command. */
function withCommand(c: CandidateView): CandidateView {
  return { ...c, tool_command: simctl };
}

describe("filters", () => {
  const candidates = [
    candidate("/a", "quarantine", "1"),
    candidate("/b", "review", "1"),
    candidate("/c", "review", "1"),
    withCommand(candidate("/d", "review", "1")),
    withCommand(candidate("/e", "keep", "1")),
  ];

  it("count and pick candidates by status, apart from items with their own command", () => {
    expect(filterCounts(candidates)).toEqual({
      all: 5,
      quarantine: 1,
      command: 1,
      review: 2,
      keep: 1,
    });
    expect(candidates.filter((c) => shows("review", c)).map((c) => c.path)).toEqual(["/b", "/c"]);
    expect(candidates.filter((c) => shows("command", c)).map((c) => c.path)).toEqual(["/d"]);
    expect(candidates.filter((c) => shows("keep", c)).map((c) => c.path)).toEqual(["/e"]);
    expect(candidates.filter((c) => shows("all", c))).toHaveLength(5);
  });

  it("read only known filters from the URL", () => {
    expect(cleanUpSearch({ show: "review" })).toEqual({ show: "review" });
    expect(cleanUpSearch({ show: "command" })).toEqual({ show: "command" });
    expect(cleanUpSearch({ show: "all" })).toEqual({});
    expect(cleanUpSearch({ show: "everything" })).toEqual({});
    expect(cleanUpSearch({ show: ["keep"] })).toEqual({});
    expect(cleanUpSearch({})).toEqual({});
  });
});

describe("cleanUpStep", () => {
  const idle = { scanning: false, scanned: false, assessment: "pending", done: false } as const;

  it("walks from start through scanning and deciding to results", () => {
    expect(cleanUpStep(idle)).toBe("start");
    expect(cleanUpStep({ ...idle, scanning: true })).toBe("scanning");
    expect(cleanUpStep({ ...idle, scanned: true })).toBe("deciding");
    expect(cleanUpStep({ ...idle, scanned: true, assessment: "success" })).toBe("results");
    expect(cleanUpStep({ ...idle, scanned: true, assessment: "error" })).toBe("decideFailed");
  });

  it("shows a finished clean-up first, and a new scan over old results", () => {
    expect(cleanUpStep({ ...idle, scanned: true, assessment: "success", done: true })).toBe("done");
    expect(cleanUpStep({ ...idle, scanning: true, scanned: true, assessment: "success" })).toBe(
      "scanning",
    );
  });
});

describe("coverageOf", () => {
  const root: RootView = {
    root: "/Users/ana",
    files: "10",
    logical_bytes: "1",
    allocated_bytes: "1",
    reclaimable_bytes: "0",
    reclaimable_exact: true,
    coverage_complete: true,
    denied: "0",
    failed: "0",
    not_scanned: "0",
  };
  const mac = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15";
  const windows = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36";

  it("is quiet when everything was read", () => {
    expect(coverageOf([root], mac)).toEqual({ incomplete: false, macPrivacy: false });
  });

  it("points to Full Disk Access only for denied folders on a Mac", () => {
    const denied = { ...root, coverage_complete: false, denied: "3" };
    expect(coverageOf([root, denied], mac)).toEqual({ incomplete: true, macPrivacy: true });
    expect(coverageOf([denied], windows)).toEqual({ incomplete: true, macPrivacy: false });
    const failed = { ...root, coverage_complete: false, failed: "2" };
    expect(coverageOf([failed], mac)).toEqual({ incomplete: true, macPrivacy: false });
  });
});

describe("reportSummary", () => {
  it("counts what moved and lists what did not", () => {
    const report: QuarantineReportView = {
      plan: "p",
      resolved: "0",
      moved_bytes: "10",
      warnings: [],
      items: [
        { path: "/a", reclaimable_bytes: "10", result: { kind: "quarantined", operation: "o" } },
        { path: "/b", reclaimable_bytes: "5", result: { kind: "skipped", reason: "changed" } },
        {
          path: "/c",
          reclaimable_bytes: "5",
          result: { kind: "failed", reason: "move_failed", left: "source" },
        },
      ],
    };
    const summary = reportSummary(report);
    expect(summary.moved).toBe(1);
    expect(summary.notMoved.map((item) => item.path)).toEqual(["/b", "/c"]);
  });

  it("adds up only kept items that quitting their apps would let Lumen propose", () => {
    const inUse = (path: string, bytes: string, processes: string[]): CandidateView => ({
      ...candidate(path, "keep", bytes),
      free_when_closed: { processes },
    });
    expect(closingSummary([candidate("/a", "keep", "9"), candidate("/b", "quarantine", "9")])).toBe(
      null,
    );
    const summary = closingSummary([
      inUse("/spotify", "1100000000", ["Spotify"]),
      inUse("/discord", "800000000", ["Spotify", "Discord"]),
      inUse("/unnamed", "1", []),
      candidate("/kept", "keep", "5000000000"),
      { ...inUse("/proposed", "7", ["Arc"]), verdict: "quarantine" },
    ]);
    expect(summary).toEqual({ apps: ["Discord", "Spotify"], bytes: "1900000001", count: 3 });
  });
});
