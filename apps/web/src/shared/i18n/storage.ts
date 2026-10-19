import { type Locale, parseLocale } from "./locale";

const KEY = "lumen.locale";

// Storage is a convenience: it can be missing, blocked or full (private
// windows, webview settings), so every access may throw and is allowed to.

/** The locale the user chose earlier, if it can be read. */
export function storedLocale(): Locale | null {
  try {
    return parseLocale(localStorage.getItem(KEY));
  } catch {
    return null;
  }
}

/** Remembers the user's choice, if storage allows it. */
export function storeLocale(locale: Locale): void {
  try {
    localStorage.setItem(KEY, locale);
  } catch {
    // Not remembered; the choice still applies until the app closes.
  }
}
