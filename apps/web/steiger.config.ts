import fsd from "@feature-sliced/steiger-plugin";
import { defineConfig } from "steiger";

// Feature-Sliced Design v2.1 rules for `src` (ADR-0009).
export default defineConfig([...fsd.configs.recommended]);
