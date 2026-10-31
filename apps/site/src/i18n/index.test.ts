import { describe, expect, it } from "vitest";
import { parseClientStrings } from "./client";
import { LOCALE_NAMES, LOCALES, localePath, messages } from "./index";

describe("site locales", () => {
  it("provides complete catalogs and valid client strings", () => {
    expect(LOCALES).toEqual(["ko", "en", "ru", "ja", "zh-CN", "es"]);
    for (const locale of LOCALES) {
      expect(messages[locale].lang).toBe(locale);
      expect(LOCALE_NAMES[locale]).not.toBe("");
      expect(parseClientStrings(JSON.stringify(messages[locale].client))).toEqual(
        messages[locale].client,
      );
    }
  });

  it("keeps Korean at the root and gives every other language its own route", () => {
    for (const locale of LOCALES) {
      const path = localePath(locale, "/Lumen/");
      expect(path).toBe(locale === "ko" ? "/Lumen/" : `/Lumen/${locale}/`);
      expect(localePath(locale, "/Lumen")).toBe(path);
    }
  });
});
