// Generates the local API's types and Zod schemas from the committed
// OpenAPI description (ADR-0011). Run `pnpm --filter @lumen/api-client
// generate` after `cargo xtask schema`; `gen-check` regenerates into a
// scratch folder and fails if it differs from `src/gen`.
import { defineConfig } from "@hey-api/openapi-ts";

export default defineConfig({
  input: "../../schemas/openapi.json",
  output: {
    path: process.env["API_CLIENT_OUT"] ?? "src/gen",
    // Formatted with the workspace's Biome settings. The drift check's
    // scratch folder is git-ignored, so the ignore file is not consulted.
    postProcess: [
      {
        name: "Biome (Format)",
        command: "biome",
        args: ["format", "--write", "--vcs-use-ignore-file=false", "{{path}}"],
      },
    ],
  },
  plugins: ["@hey-api/typescript", "zod"],
});
