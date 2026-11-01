import { describe, expect, it } from "vitest";
import { readPermille } from "./resolving";

const disk = { total_bytes: "1000", available_bytes: "200" };

describe("readPermille", () => {
  it("is the share of used space read so far", () => {
    expect(readPermille(disk, "0")).toBe(0);
    expect(readPermille(disk, "200")).toBe(250);
  });

  it("stops at all of used space", () => {
    expect(readPermille(disk, "5000")).toBe(1000);
  });

  it("is unknown without the disk's usage", () => {
    expect(readPermille(null, "200")).toBeNull();
    expect(readPermille({ total_bytes: "1000", available_bytes: "1000" }, "1")).toBeNull();
    expect(readPermille({ total_bytes: "0", available_bytes: "5" }, "1")).toBeNull();
  });
});
