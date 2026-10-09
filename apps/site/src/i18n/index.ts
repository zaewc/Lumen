import { en } from "./en";
import { ko } from "./ko";
import type { Locale, Messages } from "./types";

export const messages: Readonly<Record<Locale, Messages>> = { ko, en };

/** Korean is the default language, at the site's root; English is under en/. */
export function localePath(locale: Locale, base: string): string {
  const root = base.endsWith("/") ? base : `${base}/`;
  return locale === "ko" ? root : `${root}en/`;
}

export function otherLocale(locale: Locale): Locale {
  return locale === "ko" ? "en" : "ko";
}
