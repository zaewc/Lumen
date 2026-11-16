import { describe, expect, it } from "vitest";
import { setupSteps } from "./steps";

describe("first-launch setup steps", () => {
  it("asks about AI review first, then the helper, then Full Disk Access", () => {
    expect(setupSteps({ helper: true, disk: true })).toEqual(["ai", "helper", "disk"]);
  });

  it("skips what the host cannot offer", () => {
    expect(setupSteps({ helper: false, disk: true })).toEqual(["ai", "disk"]);
    expect(setupSteps({ helper: false, disk: false })).toEqual(["ai"]);
  });
});
