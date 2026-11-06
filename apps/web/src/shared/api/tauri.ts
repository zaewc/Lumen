// The desktop adapter of LumenTransport (ADR-0004, ADR-0009): Tauri commands,
// one per operation. Responses come from the host, but are still validated
// with the generated schemas before the UI uses them (ADR-0011).
import {
  zAiSettingsView,
  zAssessProgressView,
  zCandidateView,
  zCompareOutcomeView,
  zCompareProgressView,
  zDiskView,
  zFinalizeOutcomeView,
  zHeldItemView,
  zPlanView,
  zQuarantineReportView,
  zRestoreOutcomeView,
  zReviewReasonsView,
  zScanProgressView,
  zScanView,
  zToolRunView,
  zToolStorageView,
  zUpdateView,
} from "@lumen/api-client";
import { Channel, type InvokeArgs, invoke } from "@tauri-apps/api/core";
import { hostError } from "./host-error";
import type { LumenTransport } from "./transport";

// Older hosts omit this additive descriptor. Normalize it at the boundary so
// Zod's optional output also satisfies exactOptionalPropertyTypes.
const zCompatibleCandidate = zCandidateView.extend({
  review_reasons: zReviewReasonsView.nullable().default(null),
});
const zCompatiblePlan = zPlanView.extend({ items: zCompatibleCandidate.array() });

/** `invoke`, with a rejection turned into an `Error` that carries the host's message. */
async function call(command: string, args?: InvokeArgs): Promise<unknown> {
  try {
    return await invoke<unknown>(command, args);
  } catch (rejection) {
    throw hostError(rejection);
  }
}

/** A transport that calls the desktop host. */
export function tauriTransport(): LumenTransport {
  return {
    scan: async (request, onProgress) => {
      const progress = new Channel<unknown>();
      progress.onmessage = (message) => {
        // Progress is advisory: a malformed message is dropped, never shown.
        const parsed = zScanProgressView.safeParse(message);
        if (parsed.success) {
          onProgress?.(parsed.data);
        }
      };
      return zScanView.parse(await call("scan", { request, progress }));
    },
    assess: async (onProgress) => {
      const progress = new Channel<unknown>();
      progress.onmessage = (message) => {
        const parsed = zAssessProgressView.safeParse(message);
        if (parsed.success) onProgress?.(parsed.data);
      };
      return zCompatibleCandidate.array().parse(await call("assess", { progress }));
    },
    plan: async () => zCompatiblePlan.parse(await call("plan")),
    execute: async (request) => zQuarantineReportView.parse(await call("execute", { request })),
    held: async () => zHeldItemView.array().parse(await call("held")),
    restore: async (operationId) =>
      zRestoreOutcomeView.parse(await call("restore", { operationId })),
    finalize: async (operationId) =>
      zFinalizeOutcomeView.parse(await call("finalize", { operationId })),
    aiSettings: async () => zAiSettingsView.parse(await call("ai_settings")),
    disk: async () => zDiskView.parse(await call("disk")),
    openFullDiskAccess: async () => {
      await call("open_full_disk_access");
    },
    setTheme: async (theme) => {
      await call("set_theme", { theme });
    },
    checkUpdate: async () => zUpdateView.parse(await call("check_update")),
    installUpdate: async () => {
      await call("install_update");
    },
    runToolCommand: async (entry) => zToolRunView.parse(await call("run_tool_command", { entry })),
    toolStorage: async () => zToolStorageView.array().parse(await call("tool_storage")),
    compareFolders: async (onProgress) => {
      const progress = new Channel<unknown>();
      progress.onmessage = (message) => {
        const parsed = zCompareProgressView.safeParse(message);
        if (parsed.success) onProgress?.(parsed.data);
      };
      return zCompareOutcomeView.parse(await call("compare_folders", { progress }));
    },
    cancelComparison: async () => {
      await call("cancel_comparison");
    },
    setAiReview: async (enabled) => zAiSettingsView.parse(await call("set_ai_review", { enabled })),
    setAiKey: async (key) => zAiSettingsView.parse(await call("set_ai_key", { key })),
  };
}
