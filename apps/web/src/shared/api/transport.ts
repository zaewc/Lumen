// The UI's only way to reach the core (ADR-0009): a port with one adapter per
// surface. UI code never builds URLs or calls Tauri directly, and every
// response is validated against the generated schemas before use.
import type {
  AiSettingsView,
  CandidateView,
  DiskView,
  ExecuteRequest,
  FinalizeOutcomeView,
  HeldItemView,
  PlanView,
  QuarantineReportView,
  RestoreOutcomeView,
  ScanProgressView,
  ScanRequest,
  ScanView,
  ToolRunView,
  UpdateView,
} from "@lumen/api-client";
import type { Theme } from "@/shared/theme";

/** An operation ID, as the core issued it; never built by the UI. */
export type OperationId = string & { readonly brand: "OperationId" };

/**
 * The operation ID of a view the core returned (and the transport validated).
 * The only place a plain string becomes an `OperationId`.
 */
export function operationOf(view: { readonly operation: string }): OperationId {
  return view.operation as OperationId;
}

/** What the UI can ask of the core. Mutations name plans and operations, never paths. */
export interface LumenTransport {
  /**
   * Scans the chosen roots (none: the whole disk), read-only; `onProgress`
   * receives the totals a few times a second while it runs.
   */
  scan(request: ScanRequest, onProgress?: (progress: ScanProgressView) => void): Promise<ScanView>;
  /** Decides the latest completed scan; moves nothing. */
  assess(): Promise<readonly CandidateView[]>;
  /** Plans every candidate proposed for quarantine; moves nothing. */
  plan(): Promise<PlanView>;
  /** Executes one plan, confirmed by its full hash. */
  execute(request: ExecuteRequest): Promise<QuarantineReportView>;
  /** What quarantine holds. */
  held(): Promise<readonly HeldItemView[]>;
  /** Moves one item back, never over anything. */
  restore(operation: OperationId): Promise<RestoreOutcomeView>;
  /** Removes one item for good, only if nothing uses it. */
  finalize(operation: OperationId): Promise<FinalizeOutcomeView>;
  /** Whether AI review is on, and which key is stored (never the key). */
  aiSettings(): Promise<AiSettingsView>;
  /** How full the disk holding the home folder is. */
  disk(): Promise<DiskView>;
  /**
   * Opens System Settings at Full Disk Access; only where the host can (the
   * macOS desktop app), absent elsewhere.
   */
  openFullDiskAccess?(): Promise<void>;
  /** Sets the desktop window appearance; absent in a browser. */
  setTheme?(theme: Theme): Promise<void>;
  /** Turns AI review on (the user's consent; needs a stored key) or off. */
  setAiReview(enabled: boolean): Promise<AiSettingsView>;
  /** Stores a Jeb API key in the system's secret store; it is never returned. */
  setAiKey(key: string): Promise<AiSettingsView>;
  /**
   * The app's version and a newer published one, if any (ADR-0048); only
   * where the host can update itself (the desktop app), absent elsewhere.
   */
  checkUpdate?(): Promise<UpdateView>;
  /** Downloads, verifies and installs the newer version, then restarts. */
  installUpdate?(): Promise<void>;
  /**
   * Runs the cleanup command of a knowledge-base entry the latest
   * assessment found (ADR-0049); the host looks the command up itself.
   * Only where the host can run it (the desktop app).
   */
  runToolCommand?(entry: string): Promise<ToolRunView>;
}
