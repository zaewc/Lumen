import { defineConfig } from "astro/config";

// The download site (ADR-0038): static pages for GitHub Pages at
// https://zaewc.github.io/Lumen/. Nothing is inlined, so the pages'
// Content-Security-Policy needs no 'unsafe-inline', hashes or nonces.
export default defineConfig({
  site: "https://zaewc.github.io",
  base: "/Lumen",
  output: "static",
  trailingSlash: "always",
  build: {
    format: "directory",
    // Styles go to a .css file, never into <style> (CSP style-src 'self').
    inlineStylesheets: "never",
  },
  // The pages ship their own <meta> policy (src/layouts/Base.astro) rather
  // than Astro's security.csp, which hashes inline content this site has none of.
  devToolbar: { enabled: false },
  vite: {
    build: {
      // Astro inlines a processed <script> smaller than this limit; 0 keeps
      // every script a file (CSP script-src 'self').
      assetsInlineLimit: 0,
    },
  },
});
