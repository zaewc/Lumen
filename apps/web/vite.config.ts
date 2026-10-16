import { fileURLToPath } from "node:url";
import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

// The SPA is served by the Tauri custom protocol or the loopback dashboard
// (ADR-0009, ADR-0010); it never needs a dev server in production.
export default defineConfig({
  plugins: [react(), tailwindcss()],
  resolve: {
    alias: { "@": fileURLToPath(new URL("./src", import.meta.url)) },
  },
  build: {
    target: "es2024",
    sourcemap: true,
  },
  test: {
    environment: "node",
  },
});
