import { describe, expect, it } from "vitest";
import { isApiKey } from "./apiKey";

describe("isApiKey", () => {
  it("accepts visible ASCII, trimmed, up to 256 characters", () => {
    expect(isApiKey("sk-or-v1-abc123")).toBe(true);
    expect(isApiKey("  jeb-key\n")).toBe(true);
    expect(isApiKey("x".repeat(256))).toBe(true);
  });

  it("refuses empty, spaced, non-ASCII or overlong text", () => {
    for (const text of ["", "   ", "has space", "tab\there", "키", "x".repeat(257)]) {
      expect(isApiKey(text), JSON.stringify(text)).toBe(false);
    }
  });
});
