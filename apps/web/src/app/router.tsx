import { createRootRoute, createRoute, createRouter, Link, Outlet } from "@tanstack/react-router";
import { HomePage } from "@/pages/home";
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
      </nav>
      <Outlet />
    </>
  );
}

// Routes live in the app layer and only render pages (ADR-0009).
const root = createRootRoute({ component: Layout });
const home = createRoute({ getParentRoute: () => root, path: "/", component: HomePage });
const scan = createRoute({ getParentRoute: () => root, path: "/scan", component: ScanPage });

export const router = createRouter({ routeTree: root.addChildren([home, scan]) });

declare module "@tanstack/react-router" {
  interface Register {
    router: typeof router;
  }
}
