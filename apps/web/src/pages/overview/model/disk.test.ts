import { describe, expect, it } from "vitest";
import { diskUsage, percentText } from "./disk";

describe("diskUsage", () => {
  it("works out used space and its share exactly", () => {
    const usage = diskUsage({ total_bytes: "494384795648", available_bytes: "15032385536" });
    expect(usage.used).toBe("479352410112");
    expect(usage.available).toBe("15032385536");
    expect(usage.usedPermille).toBe(969);
    expect(usage.low).toBe(true);
  });

  it("is not low with a tenth or more available", () => {
    expect(diskUsage({ total_bytes: "1000", available_bytes: "100" }).low).toBe(false);
    expect(diskUsage({ total_bytes: "1000", available_bytes: "99" }).low).toBe(true);
  });

  it("stays exact beyond what a JavaScript number holds", () => {
    const usage = diskUsage({
      total_bytes: "18446744073709551615",
      available_bytes: "9223372036854775807",
    });
    expect(usage.used).toBe("9223372036854775808");
    expect(usage.usedPermille).toBe(500);
  });

  it("copes with an empty disk and an odd report", () => {
    expect(diskUsage({ total_bytes: "0", available_bytes: "0" })).toEqual({
      total: "0",
      used: "0",
      available: "0",
      usedPermille: 0,
      low: false,
    });
    const odd = diskUsage({ total_bytes: "100", available_bytes: "250" });
    expect(odd.available).toBe("100");
    expect(odd.usedPermille).toBe(0);
  });

  it("refuses a count that is not decimal", () => {
    expect(() => diskUsage({ total_bytes: "1e9", available_bytes: "0" })).toThrow();
  });
});

describe("percentText", () => {
  it("shows one decimal", () => {
    expect(percentText(969)).toBe("96.9");
    expect(percentText(1000)).toBe("100.0");
    expect(percentText(0)).toBe("0.0");
  });
});
