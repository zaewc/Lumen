import type { CandidateView, QuarantineReportView, RootView } from "@lumen/api-client";
import { describe, expect, it } from "vitest";
import { coverageOf, isMac } from "./coverage";
import { reportSummary } from "./report";
import { cleanUpStep } from "./step";
import { summarize } from "./summary";

function candidate(path: string, verdict: CandidateView["verdict"], bytes: string): CandidateView {
  return { path, verdict, reclaimable_bytes: bytes, reasons: [], tool_command: null };
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

  it("knows a Mac from an iPad", () => {
    expect(isMac(mac)).toBe(true);
    expect(isMac("Mozilla/5.0 (iPad; CPU OS 17_0 like Mac OS X)")).toBe(false);
    expect(isMac(windows)).toBe(false);
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
