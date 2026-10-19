import { describe, expect, it } from "vitest";
import { frameStats } from "./frameProbe";

describe("frameStats", () => {
  it("summarizes frame intervals", () => {
    const stats = frameStats([16, 16, 17.4, 16, 50]);
    expect(stats.frames).toBe(5);
    expect(stats.p50).toBe(16);
    expect(stats.p95).toBe(50);
    // 17.4 ms is vsync jitter, not a dropped frame.
    expect(stats.dropped).toBeCloseTo(1 / 5);
    expect(frameStats([]).frames).toBe(0);
  });
});
