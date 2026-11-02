import { describe, expect, it } from "vitest";
import { assessmentFailed } from "./lastScan";

describe("assessmentFailed", () => {
  const scan = {};

  it("tells a failed assessment apart from no scan and from a running one", () => {
    expect(assessmentFailed({ scanning: false, scan, assessment: "error" })).toBe(true);
    expect(assessmentFailed({ scanning: false, scan: null, assessment: "error" })).toBe(false);
    expect(assessmentFailed({ scanning: false, scan, assessment: "pending" })).toBe(false);
    expect(assessmentFailed({ scanning: false, scan, assessment: "success" })).toBe(false);
    expect(assessmentFailed({ scanning: true, scan, assessment: "error" })).toBe(false);
  });
});
