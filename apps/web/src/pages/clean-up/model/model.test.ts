import type { QuarantineReportView, RootView } from "@lumen/api-client";
import { describe, expect, it } from "vitest";
import { coverageOf } from "./coverage";
import { reportSummary } from "./report";
import { cleanUpStep } from "./step";

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
