import { describe, expect, it } from "vitest";
import { formatBytes } from "./bytes";

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
