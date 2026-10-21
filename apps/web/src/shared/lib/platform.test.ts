import { describe, expect, it } from "vitest";
import { isMac } from "./platform";

describe("isMac", () => {
  it("knows a Mac from an iPad and from Windows", () => {
    expect(isMac("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15")).toBe(
      true,
    );
    expect(isMac("Mozilla/5.0 (iPad; CPU OS 17_0 like Mac OS X)")).toBe(false);
    expect(isMac("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")).toBe(false);
  });
});
