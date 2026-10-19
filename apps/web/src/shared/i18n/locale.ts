/** The languages the UI is translated into. */
export const LOCALES = ["en", "ko"] as const;

export type Locale = (typeof LOCALES)[number];

/** Each language in its own words, for the switcher. */
export const LOCALE_NAMES: { readonly [L in Locale]: string } = {
  en: "English",
  ko: "한국어",
};

/** A locale named exactly by `value`, or `null`. */
export function parseLocale(value: string | null): Locale | null {
  return LOCALES.find((locale) => locale === value) ?? null;
}

/**
 * The first supported language in the user's preference order (BCP 47 tags,
 * as in `navigator.languages`): `ko` and `ko-*` mean Korean, `en` and `en-*`
 * English. Anything else is skipped; English is the default.
 */
export function detectLocale(languages: readonly string[]): Locale {
  for (const tag of languages) {
    const language = tag.toLowerCase().split("-")[0] ?? "";
    const locale = parseLocale(language);
    if (locale !== null) {
      return locale;
    }
  }
  return "en";
}
