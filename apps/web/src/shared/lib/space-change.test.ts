import type { SpaceChangeView } from "@lumen/api-client";
import { describe, expect, it } from "vitest";
import { summarizeSpaceChanges } from "./space-change";

function reading(volume: string, before: string, after: string): SpaceChangeView {
  return { volume, before_available_bytes: before, after_available_bytes: after };
}

describe("observed space changes", () => {
  it("compares the first and last readings instead of adding overlapping intervals", () => {
    const result = summarizeSpaceChanges([reading("a", "100", "150"), reading("a", "120", "160")]);
    expect(result).toEqual({ readings: [reading("a", "100", "160")], delta: 60n });
  });

  it("keeps disks separate and includes decreases at full integer precision", () => {
    const result = summarizeSpaceChanges([
      reading("a", "9007199254740993", "9007199254741000"),
      reading("b", "100", "80"),
    ]);
    expect(result?.readings).toHaveLength(2);
    expect(result?.delta).toBe(-13n);
  });

  it("distinguishes measured zero from missing or invalid measurements", () => {
    expect(summarizeSpaceChanges([reading("a", "100", "100")])?.delta).toBe(0n);
    expect(summarizeSpaceChanges(null)).toBeNull();
    expect(summarizeSpaceChanges([])).toBeNull();
    expect(
      summarizeSpaceChanges([reading("a", "100", "150"), reading("b", "?", "200")]),
    ).toBeNull();
  });
});
