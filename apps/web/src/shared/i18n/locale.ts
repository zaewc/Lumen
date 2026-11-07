/** The languages the UI is translated into. */
export const LOCALES = ["ko", "en", "ru", "ja", "zh-CN", "es", "fr"] as const;

export type Locale = (typeof LOCALES)[number];

/** Each language in its own words, for the switcher. */
export const LOCALE_NAMES: { readonly [L in Locale]: string } = {
  ko: "한국어",
  en: "English",
  ru: "Русский",
  ja: "日本語",
  "zh-CN": "简体中文",
  es: "Español",
  fr: "Français",
};

/** A locale named exactly by `value`, or `null`. */
export function parseLocale(value: string | null): Locale | null {
  return LOCALES.find((locale) => locale === value) ?? null;
}

/**
 * The first supported language in the user's preference order (BCP 47 tags,
 * as in `navigator.languages`). Anything else is skipped; English is the
 * default. Generic Chinese and Simplified Chinese tags use the `zh-CN` catalog,
 * but Traditional Chinese tags are not treated as Simplified Chinese.
 */
export function detectLocale(languages: readonly string[]): Locale {
  for (const tag of languages) {
    const [language = "", region] = tag.toLowerCase().split("-");
    if (
      language === "zh" &&
      (region === undefined ||
        region === "cn" ||
        region === "sg" ||
        region === "my" ||
        region === "hans")
    ) {
      return "zh-CN";
    }
    const locale = parseLocale(language);
    if (locale !== null) {
      return locale;
    }
  }
  return "en";
}
