import { createContext, type ReactNode, useContext, useEffect, useMemo, useState } from "react";
import type { Messages } from "./en";
import { detectLocale, type Locale } from "./locale";
import { MESSAGES } from "./messages";
import { storedLocale, storeLocale } from "./storage";

/** The UI language and its catalog. */
export interface I18n {
  readonly locale: Locale;
  readonly m: Messages;
  /** Switches the language and remembers the choice. */
  readonly setLocale: (locale: Locale) => void;
}

const I18nContext = createContext<I18n | null>(null);

function initialLocale(): Locale {
  return (
    storedLocale() ?? detectLocale(typeof navigator === "undefined" ? [] : navigator.languages)
  );
}

/** Provides the remembered (or the browser's preferred) language. */
export function I18nProvider({ children }: { readonly children: ReactNode }) {
  const [locale, setLocale] = useState(initialLocale);
  useEffect(() => {
    document.documentElement.lang = locale;
  }, [locale]);
  const value = useMemo<I18n>(
    () => ({
      locale,
      m: MESSAGES[locale],
      setLocale: (next) => {
        storeLocale(next);
        setLocale(next);
      },
    }),
    [locale],
  );
  return <I18nContext value={value}>{children}</I18nContext>;
}

/** The current language; only valid inside the app's providers. */
export function useI18n(): I18n {
  const i18n = useContext(I18nContext);
  if (i18n === null) {
    throw new Error("useI18n outside the app's providers");
  }
  return i18n;
}
