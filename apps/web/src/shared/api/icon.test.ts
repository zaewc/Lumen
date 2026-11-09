import { describe, expect, it } from "vitest";
import { MAX_ICON_URL, zAppIcon } from "./icon";

describe("app icon", () => {
  it("accepts a PNG data URL, or none", () => {
    expect(zAppIcon.parse("data:image/png;base64,iVBORw0KGgo=")).toBe(
      "data:image/png;base64,iVBORw0KGgo=",
    );
    expect(zAppIcon.parse(null)).toBeNull();
  });

  it("refuses anything that is not an inline PNG of bounded size", () => {
    for (const icon of [
      "data:image/svg+xml;base64,PHN2Zz4=",
      "https://example.com/icon.png",
      "javascript:alert(1)",
      "data:image/png;base64,",
      "data:image/png;base64,iVBOR w0KGgo=",
      `data:image/png;base64,${"A".repeat(MAX_ICON_URL)}`,
      "/Applications/Zed.app",
      3,
    ]) {
      expect(zAppIcon.safeParse(icon).success).toBe(false);
    }
  });
});
