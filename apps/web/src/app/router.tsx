import {
  createRootRoute,
  createRoute,
  createRouter,
  Outlet,
  redirect,
} from "@tanstack/react-router";
import { CleanUpPage } from "@/pages/clean-up";
import { QuarantinePage } from "@/pages/quarantine";
import { Header } from "@/widgets/header";

function Layout() {
  return (
    <div className="min-h-dvh bg-neutral-50 text-neutral-900">
      <Header />
      <Outlet />
    </div>
  );
}

// Routes live in the app layer and only render pages (ADR-0009).
const root = createRootRoute({ component: Layout });
const cleanUp = createRoute({ getParentRoute: () => root, path: "/", component: CleanUpPage });
const quarantine = createRoute({
  getParentRoute: () => root,
  path: "/quarantine",
  component: QuarantinePage,
});
// The former Scan and Plan pages are now steps of Clean up.
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
  routeTree: root.addChildren([cleanUp, quarantine, formerScan, formerPlan]),
});

declare module "@tanstack/react-router" {
  interface Register {
    router: typeof router;
  }
}
