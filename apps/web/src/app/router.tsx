import {
  createRootRoute,
  createRoute,
  createRouter,
  Outlet,
  redirect,
} from "@tanstack/react-router";
import { useState } from "react";
import { useAiSettings } from "@/entities/ai-review";
import { CleanUpPage, cleanUpSearch } from "@/pages/clean-up";
import { FirstLaunchPage } from "@/pages/first-launch";
import { QuarantinePage } from "@/pages/quarantine";
import { SettingsPage } from "@/pages/settings";
import { Sidebar } from "@/widgets/sidebar";
import { QuitHeld } from "./QuitHeld";

/**
 * A workspace navigation above the independently scrolling page. Storage
 * and evidence get the full window width. Until the user has chosen whether
 * to use AI review, the first screen is the first-launch setup instead
 * (ADR-0050, ADR-0067), and it stays until its last step even though the AI
 * choice is stored at its first; while the setting is being read, only the
 * empty window shows; if it cannot be read, the app opens as usual and AI
 * review stays off.
 */
function Layout() {
  const ai = useAiSettings();
  const [setup, setSetup] = useState<"none" | "open" | "done">("none");
  if (setup === "none" && ai.data !== undefined && !ai.data.chosen) {
    setSetup("open");
  }
  // Until the setting is read, draw only the window: the workspace must not
  // show for a moment before the first-launch question replaces it.
  if (ai.isPending) {
    return <div className="h-dvh bg-canvas" />;
  }
  if (setup === "open" && ai.data !== undefined) {
    return <FirstLaunchPage settings={ai.data} onDone={() => setSetup("done")} />;
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
// Storage is one page (ADR-0069): the former Overview and Explore.
const storage = createRoute({
  getParentRoute: () => root,
  path: "/",
  validateSearch: cleanUpSearch,
  component: CleanUpPage,
});
const formerExplore = createRoute({
  getParentRoute: () => root,
  path: "/clean",
  validateSearch: cleanUpSearch,
  beforeLoad: ({ search }) => {
    throw redirect({ to: "/", search });
  },
});
const quarantine = createRoute({
  getParentRoute: () => root,
  path: "/quarantine",
  component: QuarantinePage,
});
// The installed apps are a section of Storage (ADR-0069).
const formerApps = createRoute({
  getParentRoute: () => root,
  path: "/apps",
  beforeLoad: () => {
    throw redirect({ to: "/", hash: "apps" });
  },
});
const settings = createRoute({
  getParentRoute: () => root,
  path: "/settings",
  component: SettingsPage,
});
// The former Scan and Plan pages are now steps of Storage.
const formerScan = createRoute({
  getParentRoute: () => root,
  path: "/scan",
  beforeLoad: () => {
    throw redirect({ to: "/" });
  },
});
const formerPlan = createRoute({
  getParentRoute: () => root,
  path: "/plan",
  beforeLoad: () => {
    throw redirect({ to: "/" });
  },
});

export const router = createRouter({
  routeTree: root.addChildren([
    storage,
    formerExplore,
    quarantine,
    formerApps,
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
