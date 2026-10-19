// The UI's only way to reach the core (ADR-0009): a port with one adapter per
// surface. UI code never builds URLs or calls Tauri directly, and every
// response is validated against the generated schemas before use.
import type {
  CandidateView,
  ExecuteRequest,
  FinalizeOutcomeView,
  HeldItemView,
  PlanView,
  QuarantineReportView,
  RestoreOutcomeView,
  ScanProgressView,
  ScanRequest,
  ScanView,
} from "@lumen/api-client";

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
}
