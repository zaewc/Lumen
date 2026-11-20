import { describe, expect, it } from "vitest";
import { partial } from "./apps-list";

describe("software list", () => {
  it("names the inventories read only in part", () => {
    expect(
      partial([
        { source: "macos.applications", state: "complete", count: 4 },
        { source: "macos.homebrew", state: "partial", count: 3 },
        { source: "macos.launchd-plists", state: "unavailable", count: 0 },
      ]),
    ).toEqual(["macos.homebrew"]);
  });
});
