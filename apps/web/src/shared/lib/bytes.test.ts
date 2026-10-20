import { describe, expect, it } from "vitest";
import { formatBytes, sumBytes } from "./bytes";

describe("formatBytes", () => {
  it("uses decimal units and truncates like the CLI", () => {
    expect(formatBytes("0")).toBe("0 B");
    expect(formatBytes("999")).toBe("999 B");
    expect(formatBytes("1500")).toBe("1.5 kB");
    expect(formatBytes("86400000000")).toBe("86.4 GB");
    expect(formatBytes("18446744073709551615")).toBe("18.4 EB");
  });

  it("refuses anything that is not a decimal count", () => {
    expect(() => formatBytes("-1")).toThrow();
    expect(() => formatBytes("1e3")).toThrow();
  });
});

describe("sumBytes", () => {
  it("adds exactly, beyond what a JavaScript number holds", () => {
    expect(sumBytes([])).toBe("0");
    expect(sumBytes(["1500", "2500"])).toBe("4000");
    expect(sumBytes(["18446744073709551615", "1"])).toBe("18446744073709551616");
  });

  it("refuses anything that is not a decimal count", () => {
    expect(() => sumBytes(["1", "-1"])).toThrow();
  });
});
