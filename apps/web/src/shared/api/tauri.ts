// The desktop adapter of LumenTransport (ADR-0004, ADR-0009): Tauri commands,
// one per operation. Responses come from the host, but are still validated
// with the generated schemas before the UI uses them (ADR-0011).
import {
  zAiSettingsView,
  zAssessProgressView,
  zCandidateView,
  zDiskView,
  zFinalizeOutcomeView,
  zHeldItemView,
  zPlanView,
  zQuarantineReportView,
  zRestoreOutcomeView,
  zScanProgressView,
  zScanView,
  zToolRunView,
  zUpdateView,
} from "@lumen/api-client";
import { Channel, invoke } from "@tauri-apps/api/core";
import type { LumenTransport } from "./transport";

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
      return zScanView.parse(await invoke("scan", { request, progress }));
    },
    assess: async (onProgress) => {
      const progress = new Channel<unknown>();
      progress.onmessage = (message) => {
        const parsed = zAssessProgressView.safeParse(message);
        if (parsed.success) onProgress?.(parsed.data);
      };
      return zCandidateView.array().parse(await invoke("assess", { progress }));
    },
    plan: async () => zPlanView.parse(await invoke("plan")),
    execute: async (request) => zQuarantineReportView.parse(await invoke("execute", { request })),
    held: async () => zHeldItemView.array().parse(await invoke("held")),
    restore: async (operationId) =>
      zRestoreOutcomeView.parse(await invoke("restore", { operationId })),
    finalize: async (operationId) =>
      zFinalizeOutcomeView.parse(await invoke("finalize", { operationId })),
    aiSettings: async () => zAiSettingsView.parse(await invoke("ai_settings")),
    disk: async () => zDiskView.parse(await invoke("disk")),
    openFullDiskAccess: async () => {
      await invoke("open_full_disk_access");
    },
    setTheme: async (theme) => {
      await invoke("set_theme", { theme });
    },
    checkUpdate: async () => zUpdateView.parse(await invoke("check_update")),
    installUpdate: async () => {
      await invoke("install_update");
    },
    runToolCommand: async (entry) =>
      zToolRunView.parse(await invoke("run_tool_command", { entry })),
    setAiReview: async (enabled) =>
      zAiSettingsView.parse(await invoke("set_ai_review", { enabled })),
    setAiKey: async (key) => zAiSettingsView.parse(await invoke("set_ai_key", { key })),
  };
}
