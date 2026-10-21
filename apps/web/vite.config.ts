import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";
import { z } from "zod";

/**
 * The app's version, from the desktop bundle's configuration (its single
 * source); "dev" if that cannot be read.
 */
function appVersion(): string {
  try {
    const config = readFileSync(
      new URL("../desktop/src-tauri/tauri.conf.json", import.meta.url),
      "utf8",
    );
    return z.object({ version: z.string() }).parse(JSON.parse(config)).version;
  } catch {
    return "dev";
  }
}

// The SPA is served by the Tauri custom protocol or the loopback dashboard
// (ADR-0009, ADR-0010); it never needs a dev server in production.
export default defineConfig({
  plugins: [react(), tailwindcss()],
  define: {
    __LUMEN_VERSION__: JSON.stringify(appVersion()),
  },
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
