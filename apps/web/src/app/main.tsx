import { RouterProvider } from "@tanstack/react-router";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { memoryTransport } from "@/shared/api";
import { Providers } from "./Providers";
import { router } from "./router";
import "./styles.css";

const element = document.getElementById("root");
if (element === null) {
  throw new Error("index.html has no #root");
}

// The desktop host (11.8) supplies the Tauri transport; until then the app
// runs on the in-memory one.
createRoot(element).render(
  <StrictMode>
    <Providers transport={memoryTransport()}>
      <RouterProvider router={router} />
    </Providers>
  </StrictMode>,
);
