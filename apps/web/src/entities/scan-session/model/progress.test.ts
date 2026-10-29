import { describe, expect, it } from "vitest";
import { stagePercent } from "./progress";

describe("assessment stage percentage", () => {
  it("keeps large counters precise and never rounds unfinished work up to 100%", () => {
    expect(stagePercent({ stage: "deciding", completed: "2", total: "3" })).toBe(66);
    expect(
      stagePercent({
        stage: "evidence",
        completed: "18446744073709551614",
        total: "18446744073709551615",
      }),
    ).toBe(99);
    expect(stagePercent({ stage: "deciding", completed: "3", total: "3" })).toBe(100);
  });
  it("does not invent a percentage for absent, empty or invalid totals", () => {
    expect(stagePercent(null)).toBeNull();
    for (const [completed, total] of [
      ["0", null],
      ["0", "0"],
      ["4", "3"],
      ["-1", "3"],
      ["1", "NaN"],
      ["1.5", "3"],
    ]) {
      expect(
        stagePercent({ stage: "graph", completed: completed ?? "", total: total ?? null }),
      ).toBeNull();
    }
  });
});
