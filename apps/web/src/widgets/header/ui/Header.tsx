import { Link } from "@tanstack/react-router";
import { useI18n } from "@/shared/i18n";

const TAB =
  "rounded-full px-3 py-1.5 text-sm font-medium whitespace-nowrap text-neutral-600 hover:bg-neutral-100 hover:text-neutral-900 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-indigo-600 aria-[current=page]:bg-indigo-50 aria-[current=page]:text-indigo-700 dark:text-neutral-400 dark:hover:bg-neutral-800 dark:hover:text-neutral-100 dark:focus-visible:outline-indigo-400 dark:aria-[current=page]:bg-indigo-950 dark:aria-[current=page]:text-indigo-200";

/** The app's name and its places (Clean up, Quarantine, Settings). */
export function Header() {
  const { m } = useI18n();
  return (
    <header className="sticky top-0 z-30 border-b border-neutral-200 bg-white/90 backdrop-blur dark:border-neutral-800 dark:bg-neutral-900/90">
      <div className="mx-auto flex max-w-3xl items-center gap-2 px-4 py-3 sm:gap-4 sm:px-6">
        <span className="text-lg font-semibold tracking-tight text-neutral-900 dark:text-neutral-100">
          Lumen
        </span>
        <nav aria-label={m.nav.label} className="flex gap-1">
          <Link to="/" activeOptions={{ exact: true }} className={TAB}>
            {m.nav.cleanUp}
          </Link>
          <Link to="/quarantine" className={TAB}>
            {m.nav.quarantine}
          </Link>
          <Link to="/settings" className={TAB}>
            {m.nav.settings}
          </Link>
        </nav>
      </div>
    </header>
  );
}
