import { createRootRoute, createRoute, createRouter, Link, Outlet } from "@tanstack/react-router";
import { HomePage } from "@/pages/home";
import { PlanPage } from "@/pages/plan";
import { ScanPage } from "@/pages/scan";
import { LOCALE_NAMES, LOCALES, useI18n } from "@/shared/i18n";

/** English / 한국어, each named in its own language. */
function LanguageSwitcher() {
  const { locale, m, setLocale } = useI18n();
  return (
    <fieldset aria-label={m.language} className="ml-auto flex gap-2">
      {LOCALES.map((option) => (
        <button
          key={option}
          type="button"
          lang={option}
          aria-pressed={option === locale}
          onClick={() => setLocale(option)}
          className="text-neutral-600 aria-pressed:font-semibold aria-pressed:text-neutral-900"
        >
          {LOCALE_NAMES[option]}
        </button>
      ))}
    </fieldset>
  );
}

function Layout() {
  const { m } = useI18n();
  return (
    <>
      <nav className="flex gap-4 border-b border-neutral-200 px-8 py-3 text-sm">
        <Link to="/" className="[&.active]:font-semibold">
          {m.nav.quarantine}
        </Link>
        <Link to="/scan" className="[&.active]:font-semibold">
          {m.nav.scan}
        </Link>
        <Link to="/plan" className="[&.active]:font-semibold">
          {m.nav.plan}
        </Link>
        <LanguageSwitcher />
      </nav>
      <Outlet />
    </>
  );
}

// Routes live in the app layer and only render pages (ADR-0009).
const root = createRootRoute({ component: Layout });
const home = createRoute({ getParentRoute: () => root, path: "/", component: HomePage });
const scan = createRoute({ getParentRoute: () => root, path: "/scan", component: ScanPage });
const plan = createRoute({ getParentRoute: () => root, path: "/plan", component: PlanPage });

export const router = createRouter({ routeTree: root.addChildren([home, scan, plan]) });

declare module "@tanstack/react-router" {
  interface Register {
    router: typeof router;
  }
}
