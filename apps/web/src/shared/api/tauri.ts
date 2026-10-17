// The desktop adapter of LumenTransport (ADR-0004, ADR-0009): Tauri commands,
// one per operation. Responses come from the host, but are still validated
// with the generated schemas before the UI uses them (ADR-0011).
import {
  zCandidateView,
  zFinalizeOutcomeView,
  zHeldItemView,
  zPlanView,
  zQuarantineReportView,
  zRestoreOutcomeView,
  zScanView,
} from "@lumen/api-client";
import { invoke } from "@tauri-apps/api/core";
import type { LumenTransport } from "./transport";

/** A transport that calls the desktop host. */
export function tauriTransport(): LumenTransport {
  return {
    scan: async (request) => zScanView.parse(await invoke("scan", { request })),
    assess: async () => zCandidateView.array().parse(await invoke("assess")),
    plan: async () => zPlanView.parse(await invoke("plan")),
    execute: async (request) => zQuarantineReportView.parse(await invoke("execute", { request })),
    held: async () => zHeldItemView.array().parse(await invoke("held")),
    restore: async (operationId) =>
      zRestoreOutcomeView.parse(await invoke("restore", { operationId })),
    finalize: async (operationId) =>
      zFinalizeOutcomeView.parse(await invoke("finalize", { operationId })),
  };
}
