import { RouterProvider } from "@tanstack/react-router";
import { isTauri } from "@tauri-apps/api/core";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { memoryTransport, tauriTransport } from "@/shared/api";
import { Providers } from "./Providers";
import { router } from "./router";
import "./styles.css";

const element = document.getElementById("root");
if (element === null) {
  throw new Error("index.html has no #root");
}

// Inside the desktop host the app talks to the core over Tauri IPC; in a
// plain browser (development) it runs on the in-memory transport, paced so
// that each step of the flow can be seen.
const transport = isTauri() ? tauriTransport() : memoryTransport({ pace: 700 });

createRoot(element).render(
  <StrictMode>
    <Providers transport={transport}>
      <RouterProvider router={router} />
    </Providers>
  </StrictMode>,
);
