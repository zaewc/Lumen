import { en } from "./en";
import { es } from "./es";
import { ja } from "./ja";
import { ko } from "./ko";
import { ru } from "./ru";
import type { Locale, Messages } from "./types";
import { zhCN } from "./zh-CN";

export const LOCALES: readonly Locale[] = ["ko", "en", "ru", "ja", "zh-CN", "es"];

export const LOCALE_NAMES: Readonly<Record<Locale, string>> = {
  ko: "한국어",
  en: "English",
  ru: "Русский",
  ja: "日本語",
  "zh-CN": "简体中文",
  es: "Español",
};

export const messages: Readonly<Record<Locale, Messages>> = {
  ko,
  en,
  ru,
  ja,
  "zh-CN": zhCN,
  es,
};

export function screenshotLocale(locale: Locale): "ko" | "en" {
  return locale === "ko" ? "ko" : "en";
}

/** Korean is the default language at the site's root; other languages use a prefix. */
export function localePath(locale: Locale, base: string): string {
  const root = base.endsWith("/") ? base : `${base}/`;
  return locale === "ko" ? root : `${root}${locale}/`;
}
