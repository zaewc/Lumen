import {
  createRootRoute,
  createRoute,
  createRouter,
  Outlet,
  redirect,
} from "@tanstack/react-router";
import { CleanUpPage, cleanUpSearch } from "@/pages/clean-up";
import { OverviewPage } from "@/pages/overview";
import { QuarantinePage } from "@/pages/quarantine";
import { SettingsPage } from "@/pages/settings";
import { Sidebar } from "@/widgets/sidebar";

/**
 * The window, as a macOS utility lays it out: the sidebar on the left, the
 * page on the right, scrolling on its own.
 */
function Layout() {
  return (
    <div className="flex h-dvh overflow-hidden bg-window text-[13px] text-ink">
      <Sidebar />
      <div className="min-w-0 flex-1 overflow-y-auto">
        <Outlet />
      </div>
    </div>
  );
}

// Routes live in the app layer and only render pages (ADR-0009).
const root = createRootRoute({ component: Layout });
const overview = createRoute({ getParentRoute: () => root, path: "/", component: OverviewPage });
const cleanUp = createRoute({
  getParentRoute: () => root,
  path: "/clean",
  validateSearch: cleanUpSearch,
  component: CleanUpPage,
});
const quarantine = createRoute({
  getParentRoute: () => root,
  path: "/quarantine",
  component: QuarantinePage,
});
const settings = createRoute({
  getParentRoute: () => root,
  path: "/settings",
  component: SettingsPage,
});
// The former Scan and Plan pages are now steps of Clean up.
const formerScan = createRoute({
  getParentRoute: () => root,
  path: "/scan",
  beforeLoad: () => {
    throw redirect({ to: "/clean" });
  },
});
const formerPlan = createRoute({
  getParentRoute: () => root,
  path: "/plan",
  beforeLoad: () => {
    throw redirect({ to: "/clean" });
  },
});

export const router = createRouter({
  routeTree: root.addChildren([overview, cleanUp, quarantine, settings, formerScan, formerPlan]),
});

declare module "@tanstack/react-router" {
  interface Register {
    router: typeof router;
  }
}
