import {
  createRootRoute,
  createRoute,
  createRouter,
  Outlet,
  redirect,
} from "@tanstack/react-router";
import { useAiSettings } from "@/entities/ai-review";
import { AppsPage } from "@/pages/apps";
import { CleanUpPage, cleanUpSearch } from "@/pages/clean-up";
import { FirstLaunchPage } from "@/pages/first-launch";
import { OverviewPage } from "@/pages/overview";
import { QuarantinePage } from "@/pages/quarantine";
import { SettingsPage } from "@/pages/settings";
import { Sidebar } from "@/widgets/sidebar";
import { QuitHeld } from "./QuitHeld";

/**
 * A workspace navigation above the independently scrolling page. Storage
 * and evidence get the full window width. Until the user has chosen whether
 * to use AI review, the first screen asks instead (ADR-0050); while the
 * setting is being read, only the empty window shows; if it cannot be read,
 * the app opens as usual and AI review stays off.
 */
function Layout() {
  const ai = useAiSettings();
  // Until the setting is read, draw only the window: the workspace must not
  // show for a moment before the first-launch question replaces it.
  if (ai.isPending) {
    return <div className="h-dvh bg-canvas" />;
  }
  if (ai.data !== undefined && !ai.data.chosen) {
    return <FirstLaunchPage settings={ai.data} />;
  }
  return (
    <div className="flex h-dvh flex-col overflow-hidden bg-canvas text-[14px] leading-relaxed text-ink">
      <Sidebar />
      <QuitHeld />
      <div className="min-h-0 min-w-0 flex-1 overflow-y-auto overscroll-contain">
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
const apps = createRoute({
  getParentRoute: () => root,
  path: "/apps",
  component: AppsPage,
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
  routeTree: root.addChildren([
    overview,
    cleanUp,
    quarantine,
    apps,
    settings,
    formerScan,
    formerPlan,
  ]),
});

declare module "@tanstack/react-router" {
  interface Register {
    router: typeof router;
  }
}
