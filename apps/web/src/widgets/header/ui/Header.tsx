import { Link } from "@tanstack/react-router";
import { LOCALE_NAMES, LOCALES, useI18n } from "@/shared/i18n";

/** English / 한국어, each named in its own language, in a small select. */
function LanguageSelect() {
  const { locale, m, setLocale } = useI18n();
  return (
    <select
      aria-label={m.language}
      value={locale}
      onChange={(event) => {
        const next = LOCALES.find((option) => option === event.target.value);
        if (next !== undefined) {
          setLocale(next);
        }
      }}
      className="ml-auto rounded-lg border border-neutral-300 bg-white py-1 pr-7 pl-2 text-sm text-neutral-700 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-indigo-600"
    >
      {LOCALES.map((option) => (
        <option key={option} value={option} lang={option}>
          {LOCALE_NAMES[option]}
        </option>
      ))}
    </select>
  );
}

const TAB =
  "rounded-full px-3 py-1.5 text-sm font-medium whitespace-nowrap text-neutral-600 hover:bg-neutral-100 hover:text-neutral-900 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-indigo-600 aria-[current=page]:bg-indigo-50 aria-[current=page]:text-indigo-700";

/** The app's name, its two places (Clean up, Quarantine), and the language. */
export function Header() {
  const { m } = useI18n();
  return (
    <header className="sticky top-0 z-30 border-b border-neutral-200 bg-white/90 backdrop-blur">
      <div className="mx-auto flex max-w-3xl items-center gap-2 px-4 py-3 sm:gap-4 sm:px-6">
        <span className="text-lg font-semibold tracking-tight text-neutral-900">Lumen</span>
        <nav aria-label={m.nav.label} className="flex gap-1">
          <Link to="/" activeOptions={{ exact: true }} className={TAB}>
            {m.nav.cleanUp}
          </Link>
          <Link to="/quarantine" className={TAB}>
            {m.nav.quarantine}
          </Link>
        </nav>
        <LanguageSelect />
      </div>
    </header>
  );
}
