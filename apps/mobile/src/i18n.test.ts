import { describe, expect, it } from "vitest";
import { detectLocale, MESSAGES } from "./i18n";

describe("mobile locale", () => {
  it("uses French for French device language tags", () => {
    expect(detectLocale("fr")).toBe("fr");
    expect(detectLocale("fr-FR")).toBe("fr");
    expect(detectLocale("fr_CA")).toBe("fr");
  });

  it("uses English for other and unsupported languages", () => {
    expect(detectLocale("en-US")).toBe("en");
    expect(detectLocale("ko-KR")).toBe("en");
  });

  it("provides translated text for both supported catalogs", () => {
    for (const locale of ["en", "fr"] as const) {
      for (const value of Object.values(MESSAGES[locale])) {
        expect(value).toBeTruthy();
      }
    }
    expect(MESSAGES.fr.similarGroupCount("1", 1)).toBe("1 groupe de photos similaires");
    expect(MESSAGES.fr.repeatFrom("50000", "120 photos")).toBe(
      "50000 éléments, répétés à partir de 120 photos",
    );
  });
});
