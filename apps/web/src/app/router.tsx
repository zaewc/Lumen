import { createRootRoute, createRoute, createRouter, Outlet } from "@tanstack/react-router";
import { HomePage } from "@/pages/home";

// Routes live in the app layer and only render pages (ADR-0009).
const root = createRootRoute({ component: Outlet });
const home = createRoute({ getParentRoute: () => root, path: "/", component: HomePage });

export const router = createRouter({ routeTree: root.addChildren([home]) });

declare module "@tanstack/react-router" {
  interface Register {
    router: typeof router;
  }
}
