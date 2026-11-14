// The UI's only way to reach the core (ADR-0009): a port with one adapter per
// surface. UI code never builds URLs or calls Tauri directly, and every
// response is validated against the generated schemas before use.
import type {
  AiSettingsView,
  AssessProgressView,
  CandidateView,
  CompareOutcomeView,
  CompareProgressView,
  DiskView,
  ExecuteRequest,
  FinalizeOutcomeView,
  HeldItemView,
  HelperView,
  PlanView,
  QuarantineReportView,
  QuitView,
  ReassessRequest,
  ReassessView,
  RestoreOutcomeView,
  ScanProgressView,
  ScanRequest,
  ScanView,
  SoftwareView,
  ToolRunView,
  ToolStorageView,
  UninstallOutcomeView,
  UninstallPreviewView,
  UnreadFoldersView,
  UpdateView,
} from "@lumen/api-client";
import type { Locale } from "@/shared/i18n";
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
  assess(onProgress?: (progress: AssessProgressView) => void): Promise<readonly CandidateView[]>;
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
  /**
   * Sets the language of the desktop host's own confirmation dialogs (FS-1);
   * absent in a browser.
   */
  setLanguage?(locale: Locale): Promise<void>;
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
  /**
   * What Xcode, Docker and the package managers report about their own
   * storage; read-only queries the host runs only when asked. Only where the
   * host can run them (the desktop app).
   */
  toolStorage?(): Promise<readonly ToolStorageView[]>;
  /**
   * The installed software list: every inventory the platform keeps of
   * installed applications and packages, read on request (ADR-0060). Read
   * only. Only where the host can read them (the desktop app).
   */
  software?(): Promise<SoftwareView>;
  /**
   * The installed software list after AI review explains software it has not
   * explained yet (ADR-0061): sends identity fields (origin, identifier, name,
   * publisher, version, linked background items) only while AI review is on,
   * at most 40 per call; otherwise the list with notes already on the device.
   * Only where the host can read the list (the desktop app).
   */
  explainSoftware?(): Promise<SoftwareView>;
  /**
   * Finds identical copies within the roots of the latest completed scan
   * (ADR-0065). The host verifies the scan ID and reads its roots from the
   * index; the UI cannot provide paths.
   */
  compareScan?(
    scan: string,
    onProgress?: (progress: CompareProgressView) => void,
  ): Promise<CompareOutcomeView>;
  /** Stops the running comparison, which then ends as `cancelled`. */
  cancelComparison?(): Promise<void>;
  /**
   * Checks items of the latest assessment again with fresh read-only
   * evidence (ADR-0058), by ID; none: every item with something unknown.
   * Moves nothing. Only where the host keeps the assessment (the desktop app).
   */
  reassess?(request: ReassessRequest): Promise<ReassessView>;
  /**
   * Previews uninstalling the macOS app the software list shows under `key`
   * (ADR-0062): ready to confirm by its hash, or every reason it cannot go to
   * Quarantine. Reads only; moves nothing. Only where the host can read the
   * list (the desktop app); off macOS it answers `unsupported`.
   */
  uninstallPreview?(key: string): Promise<UninstallPreviewView>;
  /**
   * Moves the app of one preview into Quarantine, confirmed by the preview's
   * full hash; names the preview, never a path. The host checks the app again
   * right before the move. Restorable until removed for good.
   */
  uninstall?(preview: string, confirmation: string): Promise<UninstallOutcomeView>;
  /**
   * Asks the running macOS app the software list shows under `key` to quit,
   * the way its Quit menu item does (roadmap 16.3a): it may ask to save or
   * decline, and is never force-quit. Preview it again to see whether it
   * still runs. Only where the host can (the desktop app on macOS).
   */
  quitApp?(key: string): Promise<QuitView>;
  /**
   * Lumen's helper (ADR-0063): where its registration stands and, once an
   * administrator allowed it, whether it answers. Only on macOS.
   */
  helperState?(): Promise<HelperView>;
  /** Sets the helper up; macOS then asks an administrator to allow it. */
  helperEnable?(): Promise<HelperView>;
  /** Removes the helper. */
  helperDisable?(): Promise<HelperView>;
  /** Opens System Settings at Login Items, where the helper is allowed. */
  helperOpenLoginItems?(): Promise<void>;
  /**
   * The icon of the app the software list shows under `key` (roadmap 16.4):
   * a small PNG as a `data:` URL, or `null` if there is none. Takes the
   * list's key, never a path. Display only; never sent to AI review. Only
   * where the host can draw it (the desktop app; macOS for now).
   */
  appIcon?(key: string): Promise<string | null>;
  /**
   * The folders a scan did not read in full, by path and why (ADR-0008):
   * at most 1000, with the total. Reads only the index; takes the scan ID,
   * never a path. Display only; never sent to AI review. Only where the
   * host keeps the index (the desktop app).
   */
  unreadFolders?(scan: string): Promise<UnreadFoldersView>;
}
