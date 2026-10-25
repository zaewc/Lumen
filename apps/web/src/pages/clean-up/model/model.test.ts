import type { CandidateView, QuarantineReportView, RootView } from "@lumen/api-client";
import { describe, expect, it } from "vitest";
import { coverageOf } from "./coverage";
import { cleanUpSearch, filterCounts, shows } from "./filter";
import { reportSummary } from "./report";
import { sectionsOf } from "./sections";
import { cleanUpStep } from "./step";

function candidate(
  path: string,
  verdict: CandidateView["verdict"],
  bytes: string,
  group: CandidateView["group"] = { kind: "other", source: null },
): CandidateView {
  return { path, verdict, reclaimable_bytes: bytes, reasons: [], tool_command: null, group };
}

const xcode = { kind: "developer", source: "Xcode" } as const;
const npm = { kind: "developer", source: "npm" } as const;
const unknownTool = { kind: "developer", source: null } as const;
const cache = { kind: "app_cache", source: "com.example.app" } as const;

describe("sectionsOf", () => {
  const candidates = [
    candidate("/other", "keep", "0"),
    candidate("/npm", "quarantine", "1800"),
    candidate("/cache-b", "review", "20", cache),
    candidate("/derived", "quarantine", "4200", xcode),
    candidate("/sim", "review", "3100", xcode),
    candidate("/tool", "review", "9000", unknownTool),
    candidate("/cache-a", "quarantine", "18446744073709551615", cache),
  ].map((c) => (c.path === "/npm" ? { ...c, group: npm } : c));

  it("orders kinds, leaves out empty ones and totals each exactly", () => {
    const sections = sectionsOf(candidates);
    expect(sections.map((s) => s.kind)).toEqual(["developer", "app_cache", "other"]);
    const [developer, appCache, other] = sections;
    expect(developer?.count).toBe(4);
    expect(developer?.bytes).toBe("18100");
    expect(appCache?.bytes).toBe("18446744073709551635");
    expect(other?.count).toBe(1);
  });

  it("splits developer tools by tool, largest first, the unknown tool last", () => {
    const [developer, appCache] = sectionsOf(candidates);
    expect(developer?.sources.map((s) => [s.source, s.bytes])).toEqual([
      ["Xcode", "7300"],
      ["npm", "1800"],
      [null, "9000"],
    ]);
    expect(developer?.sources[0]?.items.map((c) => c.path)).toEqual(["/derived", "/sim"]);
    expect(appCache?.sources.map((s) => s.source)).toEqual([null]);
    expect(appCache?.sources[0]?.items.map((c) => c.path)).toEqual(["/cache-a", "/cache-b"]);
  });

  it("has no sections without candidates", () => {
    expect(sectionsOf([])).toEqual([]);
  });
});

describe("filters", () => {
  const candidates = [
    candidate("/a", "quarantine", "1"),
    candidate("/b", "review", "1"),
    candidate("/c", "review", "1"),
  ];

  it("count and pick candidates by verdict", () => {
    expect(filterCounts(candidates)).toEqual({ all: 3, quarantine: 1, review: 2, keep: 0 });
    expect(candidates.filter((c) => shows("review", c)).map((c) => c.path)).toEqual(["/b", "/c"]);
    expect(candidates.filter((c) => shows("all", c))).toHaveLength(3);
  });

  it("read only known filters from the URL", () => {
    expect(cleanUpSearch({ show: "review" })).toEqual({ show: "review" });
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
          result: { kind: "failed", reason: "Io", left: "source" },
        },
      ],
    };
    const summary = reportSummary(report);
    expect(summary.moved).toBe(1);
    expect(summary.notMoved.map((item) => item.path)).toEqual(["/b", "/c"]);
  });
});
