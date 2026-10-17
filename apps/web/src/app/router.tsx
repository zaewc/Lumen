import { createRootRoute, createRoute, createRouter, Link, Outlet } from "@tanstack/react-router";
import { HomePage } from "@/pages/home";
import { PlanPage } from "@/pages/plan";
import { ScanPage } from "@/pages/scan";

function Layout() {
  return (
    <>
      <nav className="flex gap-4 border-b border-neutral-200 px-8 py-3 text-sm">
        <Link to="/" className="[&.active]:font-semibold">
          Quarantine
        </Link>
        <Link to="/scan" className="[&.active]:font-semibold">
          Scan
        </Link>
        <Link to="/plan" className="[&.active]:font-semibold">
          Plan
        </Link>
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
