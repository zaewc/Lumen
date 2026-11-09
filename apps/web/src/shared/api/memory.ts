// An in-memory LumenTransport for development and tests: fixed, plausible
// data that the tests validate with the same schemas as real responses. It
// never touches a filesystem, a keychain or the network.
import type {
  AiSettingsView,
  CandidateView,
  HeldItemView,
  HelperView,
  ItemReportView,
  PlanView,
  ScanView,
  SoftwareItemView,
  SoftwareView,
  UninstallAppView,
  UninstallPreviewView,
} from "@lumen/api-client";
import { isApiKey, sumBytes } from "@/shared/lib";
import type { LumenTransport } from "./transport";

const HOME = "/Users/ana";

// The sample software list; `explained` adds the notes AI review would
// have given for software it had not explained yet (ADR-0061).
function softwareList(explained: boolean): SoftwareView {
  const row = (
    origin: SoftwareItemView["origin"],
    id: string,
    name: string | null,
    version: string | null,
    extra: Partial<SoftwareItemView> = {},
  ): SoftwareItemView => ({
    key: `${origin}:${id}`,
    origin,
    id,
    name,
    version,
    vendor: null,
    location: null,
    protected: false,
    hidden: false,
    explicit: null,
    services: [],
    ai_note: null,
    ...extra,
  });
  return {
    items: [
      row("homebrew", "act", null, "0.2.79", {
        location: "/opt/homebrew/Cellar/act",
        explicit: true,
      }),
      row("mac_app", "com.apple.calculator", "Calculator", "11.0", {
        location: "/System/Applications/Calculator.app",
        protected: true,
      }),
      row("homebrew_cask", "docker", null, "4.30.0", {
        location: "/opt/homebrew/Caskroom/docker",
        explicit: true,
      }),
      row("mac_installer", "com.example.driver.pkg", null, "2.4.1", { location: "/" }),
      row("mac_app", "com.example.OldEditor", "OldEditor", "3.2", {
        location: "/Applications/OldEditor.app",
        services: [
          {
            kind: "launch_agent",
            label: "com.example.OldEditor.updater",
            scope: "user",
            method: "program_inside_application",
            verified: true,
            ambiguous: false,
          },
        ],
      }),
      row("homebrew", "openssl@3", null, "3.5.0", {
        location: "/opt/homebrew/Cellar/openssl@3",
        explicit: false,
        ai_note: {
          purpose: "runtime_or_library",
          removal_impact: "other_software_depends",
          purpose_confidence: "high",
          removal_impact_confidence: "medium",
        },
      }),
      row("mac_app", "com.spotify.client", "Spotify", "1.2.48", {
        location: "/Applications/Spotify.app",
        ai_note: explained
          ? {
              purpose: "media",
              removal_impact: "only_this_app",
              purpose_confidence: "high",
              removal_impact_confidence: "medium",
            }
          : null,
      }),
    ],
    sources: [
      { source: "macos.applications", state: "complete", count: 4 },
      { source: "macos.package-receipts", state: "complete", count: 1 },
      { source: "macos.homebrew", state: "complete", count: 3 },
      { source: "macos.launchd-plists", state: "complete", count: 1 },
    ],
  };
}

const scanView: ScanView = {
  scan: "0190f0d2-6c1e-7a3b-8c4d-5e6f708192a1",
  roots: [
    {
      root: `${HOME}/Library`,
      files: "1204",
      logical_bytes: "18400000000",
      allocated_bytes: "18500000000",
      reclaimable_bytes: "9900000000",
      reclaimable_exact: true,
      coverage_complete: true,
      denied: "0",
      failed: "0",
      not_scanned: "0",
    },
    {
      root: `${HOME}/Library/Containers`,
      files: "376",
      logical_bytes: "1900000000",
      allocated_bytes: "2000000000",
      reclaimable_bytes: "0",
      reclaimable_exact: false,
      coverage_complete: false,
      denied: "12",
      failed: "0",
      not_scanned: "0",
    },
  ],
  entries: "1580",
  partial: false,
  out_of_space: false,
};

/** The reason the policy gives for a regenerable developer cache. */
const regenerable = {
  rule: "classify.devkb-regenerable",
  text: "Quarantine proposed: the tool that made it can regenerate it.",
} as const;

/** When the fixture's scan finished and its assessment ran. */
const SCANNED = "2026-10-16T08:58:00Z";
const ASSESSED = "2026-10-16T09:00:00Z";

type Fact = CandidateView["facts"][number];

/** A fact as its source observed it (ADR-0053). */
function seen(fact: Fact["fact"], source: string, observedAt = SCANNED): Fact {
  return { fact, basis: { kind: "observed" }, source, observed_at: observedAt };
}

/**
 * A folder's own times: modified as recorded, accessed as it may lag, or not
 * recorded (`null`: unpacked from an archive that kept no times).
 */
function folderTimes(modified: string, accessed: string | null): CandidateView["times"] {
  return {
    modified: { state: "recorded", at: modified },
    accessed: accessed === null ? { state: "not_recorded" } : { state: "may_lag", at: accessed },
    folder: true,
  };
}

/**
 * What the scan and the probes state about every folder here: its protection
 * flags, cloud backing, whether a process uses it (`partial`: no visible
 * process does, but not every process was visible) and whether the user owns
 * it. A probe that could not answer states nothing (`null`): unknown.
 */
function scanned({
  cloud = false,
  inUse = false,
  owned = true,
}: {
  readonly cloud?: boolean;
  readonly inUse?: boolean | "partial";
  readonly owned?: boolean | null;
} = {}): readonly Fact[] {
  const flags = { system_restricted: false, undeletable: false, system_attribute: false };
  const facts: Fact[] = [
    seen({ type: "protected", ...flags, hidden: false }, "lumen.scan"),
    seen({ type: "cloud_backed", cloud_backed: cloud }, "lumen.scan"),
  ];
  if (inUse === "partial") {
    const partial: Fact["fact"] = { type: "not_in_use_by_visible_processes", unseen: "3" };
    facts.push(seen(partial, "macos.process-probe", ASSESSED));
  } else {
    facts.push(seen({ type: "in_use", in_use: inUse }, "macos.process-probe", ASSESSED));
  }
  if (owned !== null) {
    facts.push(seen({ type: "owned_by_current_user", owned }, "unix.owner", ASSESSED));
  }
  return facts;
}

/** A developer knowledge-base match. */
function devkb(entry: string): Fact {
  return seen({ type: "knowledge_base_match", entry }, "devkb");
}

/**
 * An application's data folder, named after it, and whether it is installed;
 * an installed one is not running and was last opened two days before the
 * assessment (ADR-0059: shown, never decided on).
 */
function appData(
  app: string,
  kind: "caches" | "application_support" | "web_data",
  installed: boolean,
): readonly Fact[] {
  const source = "macos.support-location.bundle-id";
  return [
    seen({ type: "owning_application", app }, source),
    seen({ type: "app_data_location", kind }, source),
    ...(installed
      ? [
          seen({ type: "application_present", app, present: true }, "macos.applications", ASSESSED),
          seen(
            { type: "application_running", app, running: false },
            "macos.process-probe",
            ASSESSED,
          ),
          seen(
            { type: "application_last_opened", app, at: "2026-10-27T09:12:00Z" },
            "macos.launchservices",
            ASSESSED,
          ),
        ]
      : [
          {
            fact: { type: "application_present", app, present: false },
            basis: { kind: "absence", searched: ["macos.applications"] },
            source: "lumen.inventory",
            observed_at: ASSESSED,
          } satisfies Fact,
        ]),
  ];
}

/** The fixture's candidates' opaque IDs, as the host makes them from file identities. */
function fileId(n: number): string {
  return `dev:1000010:${(8_400_000 + n).toString()}`;
}

/**
 * The relations in a candidate's bundle (ADR-0053): its own `owned_by` edge,
 * then those of the app's other folders, two hops away.
 */
function ownedBy(
  app: string,
  own: number,
  siblings: readonly number[] = [],
): CandidateView["relations"] {
  const edge = (n: number, hops: number): CandidateView["relations"]["edges"][number] => ({
    from: { type: "file", id: fileId(n) },
    kind: "owned_by",
    to: { type: "application", app },
    hops,
  });
  return { edges: [edge(own, 1), ...siblings.map((n) => edge(n, 2))], omitted: "0" };
}

/** No relation recorded: not the same as unrelated. */
const noneRecorded: CandidateView["relations"] = { edges: [], omitted: "0" };

/**
 * What a scan of the fixture finds: every group, every verdict, and items to
 * clean up with their tool's own command (ADR-0039).
 */
const candidates: readonly CandidateView[] = [
  {
    id: fileId(1),
    path: `${HOME}/Library/Developer/Xcode/DerivedData`,
    verdict: "quarantine",
    reclaimable_bytes: "4200000000",
    reclaimable_exact: true,
    reasons: [regenerable],
    tool_command: null,
    group: { kind: "developer", source: "Xcode" },
    ai_note: null,
    free_when_closed: null,
    times: folderTimes("2026-10-15T17:42:10Z", "2026-10-16T08:12:03Z"),
    facts: [devkb("xcode-derived-data"), ...scanned()],
    relations: noneRecorded,
  },
  {
    id: fileId(2),
    path: `${HOME}/Library/Developer/CoreSimulator/Devices`,
    verdict: "review",
    reclaimable_bytes: "3100000000",
    reclaimable_exact: true,
    reasons: [
      {
        rule: "classify.devkb-tool-action",
        text: "Review: only the tool's own cleanup command should remove it, and that cannot be undone.",
      },
    ],
    tool_command: {
      entry: "xcode-simulator-devices",
      removes_data: false,
      tool: "Xcode",
      program: "xcrun",
      args: ["simctl", "delete", "unavailable"],
    },
    group: { kind: "developer", source: "Xcode" },
    ai_note: null,
    free_when_closed: null,
    times: folderTimes("2026-09-02T11:20:00Z", "2026-10-14T19:05:44Z"),
    facts: [devkb("xcode-simulator-devices"), ...scanned()],
    relations: noneRecorded,
  },
  {
    id: fileId(3),
    path: `${HOME}/Library/pnpm/store`,
    verdict: "review",
    reclaimable_bytes: "5200000000",
    reclaimable_exact: true,
    reasons: [
      {
        rule: "classify.devkb-tool-action",
        text: "Review: only the tool's own cleanup command should remove it, and that cannot be undone.",
      },
    ],
    tool_command: {
      entry: "pnpm-store",
      removes_data: false,
      tool: "pnpm",
      program: "pnpm",
      args: ["store", "prune"],
    },
    group: { kind: "developer", source: "pnpm" },
    ai_note: null,
    free_when_closed: null,
    times: folderTimes("2026-10-11T09:31:27Z", "2026-10-11T09:31:27Z"),
    facts: [devkb("pnpm-store"), ...scanned()],
    relations: noneRecorded,
  },
  {
    id: fileId(4),
    path: `${HOME}/.gradle/caches`,
    verdict: "quarantine",
    reclaimable_bytes: "2600000000",
    reclaimable_exact: true,
    reasons: [regenerable],
    tool_command: null,
    group: { kind: "developer", source: "Gradle" },
    ai_note: null,
    free_when_closed: null,
    times: folderTimes("2026-06-30T14:02:51Z", "2026-07-01T08:40:12Z"),
    facts: [devkb("gradle-user-home-caches"), ...scanned()],
    relations: noneRecorded,
  },
  {
    id: fileId(5),
    path: `${HOME}/.npm/_cacache`,
    verdict: "quarantine",
    reclaimable_bytes: "1800000000",
    reclaimable_exact: true,
    reasons: [regenerable],
    tool_command: null,
    group: { kind: "developer", source: "npm" },
    ai_note: null,
    free_when_closed: null,
    times: folderTimes("2026-10-12T22:16:05Z", "2026-10-15T10:01:33Z"),
    facts: [devkb("npm-cache"), ...scanned()],
    relations: noneRecorded,
  },
  {
    id: fileId(6),
    path: `${HOME}/Library/Caches/go-build`,
    verdict: "keep",
    reclaimable_bytes: "890000000",
    reclaimable_exact: true,
    reasons: [
      {
        rule: "classify.devkb-self-managed",
        text: "Kept: the tool that made it cleans it up automatically.",
      },
    ],
    tool_command: null,
    group: { kind: "developer", source: "Go" },
    ai_note: null,
    free_when_closed: null,
    times: folderTimes("2026-10-16T07:55:19Z", "2026-10-16T07:55:19Z"),
    facts: [devkb("go-build-cache"), ...scanned()],
    relations: noneRecorded,
  },
  {
    id: fileId(7),
    path: `${HOME}/Library/Caches/com.spotify.client`,
    verdict: "quarantine",
    reclaimable_bytes: "1300000000",
    reclaimable_exact: true,
    reasons: [
      {
        rule: "classify.app-cache",
        text: "Quarantine proposed: it is an application's cache, which the application re-creates.",
      },
    ],
    tool_command: null,
    group: { kind: "app_cache", source: "com.spotify.client" },
    ai_note: null,
    free_when_closed: null,
    times: folderTimes("2026-10-16T08:30:02Z", "2026-10-16T08:41:57Z"),
    facts: [...appData("com.spotify.client", "caches", true), ...scanned()],
    relations: ownedBy("com.spotify.client", 7),
  },
  {
    id: fileId(8),
    path: `${HOME}/Library/Application Support/discord/Cache`,
    verdict: "keep",
    reclaimable_bytes: "1100000000",
    reclaimable_exact: true,
    reasons: [{ rule: "protect.in-use", text: "Kept: a running process is using it." }],
    tool_command: null,
    group: { kind: "app_cache", source: "com.hnc.Discord" },
    ai_note: null,
    free_when_closed: { processes: ["Discord"] },
    rechecks: ["in_use"],
    times: folderTimes("2026-10-16T08:57:40Z", "2026-10-16T08:57:40Z"),
    facts: [devkb("discord-chromium-caches"), ...scanned({ inUse: true })],
    relations: noneRecorded,
  },
  {
    id: fileId(9),
    path: `${HOME}/Library/Caches/Google/Chrome`,
    verdict: "review",
    reclaimable_bytes: "760000000",
    reclaimable_exact: true,
    reasons: [
      {
        rule: "classify.app-cache",
        text: "Quarantine proposed: it is an application's cache, which the application re-creates.",
      },
      { rule: "jev.demote", text: "Review: the optional AI review advised caution." },
    ],
    tool_command: null,
    group: { kind: "app_cache", source: "com.google.Chrome" },
    ai_note: null,
    free_when_closed: null,
    times: folderTimes("2026-10-16T08:50:12Z", "2026-10-16T08:50:12Z"),
    facts: [...appData("com.google.Chrome", "caches", true), ...scanned({ inUse: "partial" })],
    relations: ownedBy("com.google.Chrome", 9),
  },
  {
    id: fileId(10),
    path: `${HOME}/Library/Application Support/Slack`,
    verdict: "keep",
    reclaimable_bytes: "980000000",
    reclaimable_exact: true,
    reasons: [
      {
        rule: "classify.app-state",
        text: "Kept: it holds an application's own data, settings or documents; Lumen leaves it alone.",
      },
    ],
    tool_command: null,
    group: { kind: "app_data", source: "com.tinyspeck.slackmacgap" },
    ai_note: null,
    free_when_closed: null,
    times: folderTimes("2026-10-16T08:44:09Z", "2026-10-16T08:44:09Z"),
    facts: [...appData("com.tinyspeck.slackmacgap", "application_support", true), ...scanned()],
    relations: ownedBy("com.tinyspeck.slackmacgap", 10, [12]),
  },
  {
    id: fileId(11),
    path: `${HOME}/Library/Application Support/OldEditor`,
    verdict: "review",
    reclaimable_bytes: "420000000",
    reclaimable_exact: true,
    reasons: [
      {
        rule: "classify.orphan",
        text: "Review: the application it belongs to appears to be uninstalled.",
      },
    ],
    tool_command: null,
    group: { kind: "app_data", source: "com.example.OldEditor" },
    ai_note: null,
    free_when_closed: null,
    times: folderTimes("2025-03-18T10:12:44Z", "2025-11-30T21:03:10Z"),
    facts: [
      ...appData("com.example.OldEditor", "application_support", false),
      ...scanned({ owned: null }),
    ],
    relations: ownedBy("com.example.OldEditor", 11),
  },
  {
    id: fileId(12),
    path: `${HOME}/Library/HTTPStorages/com.tinyspeck.slackmacgap`,
    verdict: "keep",
    reclaimable_bytes: "380928",
    reclaimable_exact: true,
    reasons: [{ rule: "default.keep-small", text: "Kept: too small to be worth reviewing." }],
    tool_command: null,
    group: { kind: "other", source: null },
    ai_note: null,
    free_when_closed: null,
    times: folderTimes("2026-10-16T08:44:12Z", "2026-10-16T08:44:12Z"),
    facts: [...appData("com.tinyspeck.slackmacgap", "web_data", true), ...scanned()],
    relations: ownedBy("com.tinyspeck.slackmacgap", 12, [10]),
  },
  {
    id: fileId(13),
    path: `${HOME}/Downloads/Installers`,
    review_reasons: { schema: "review-reasons/1", codes: ["unknown_owner"] },
    verdict: "review",
    reclaimable_bytes: "2300000000",
    reclaimable_exact: true,
    reasons: [{ rule: "default.review", text: "Review: Lumen does not recognise this item." }],
    tool_command: null,
    group: { kind: "other", source: null },
    ai_note: null,
    free_when_closed: null,
    times: folderTimes("2026-08-21T16:03:29Z", null),
    facts: [
      seen({ type: "large_unrecognised", reclaimable_bytes: "2300000000" }, "lumen.large-folder"),
      ...scanned(),
    ],
    relations: noneRecorded,
  },
  {
    id: fileId(14),
    path: `${HOME}/Library/Mobile Documents/com~apple~CloudDocs`,
    verdict: "keep",
    reclaimable_bytes: "0",
    reclaimable_exact: true,
    reasons: [
      {
        rule: "protect.cloud-backed",
        text: "Kept: it is synced with a cloud service, so removing it here could remove it everywhere.",
      },
      { rule: "protect.in-use", text: "Kept: a running process is using it." },
    ],
    tool_command: null,
    group: { kind: "other", source: null },
    ai_note: null,
    free_when_closed: null,
    times: folderTimes("2026-10-16T08:20:51Z", "2026-10-16T08:58:00Z"),
    facts: [...scanned({ cloud: true, inUse: true })],
    relations: noneRecorded,
  },
];

const proposed = candidates.filter((c) => c.verdict === "quarantine");

/**
 * What AI review says the unrecognised review items most likely are, once it
 * is on (ADR-0041): notes only, the verdicts stay the same.
 */
const notes: ReadonlyMap<string, NonNullable<CandidateView["ai_note"]>> = new Map([
  [`${HOME}/Library/Application Support/OldEditor`, { kind: "app_settings", confidence: "medium" }],
  [`${HOME}/Downloads/Installers`, { kind: "downloads_or_installers", confidence: "low" }],
]);

const planView: PlanView = {
  plan: "0190f0d2-6c1e-7a3b-8c4d-5e6f708192a2",
  hash: "b3:8c05bf8f00722b4a58174b715d1b11cfc4f69000c54b269df53b85cfa0cff22f",
  created_at: "2026-10-16T09:00:00Z",
  items: proposed,
};

/** The planned item that changes before the move, so it is skipped. */
const CHANGED = `${HOME}/.npm/_cacache`;

/** What quarantine holds before anything is cleaned up here. */
const initiallyHeld: readonly HeldItemView[] = [
  {
    operation: "0190f0d2-6c1e-7a3b-8c4d-5e6f708192a3",
    state: "retained",
    original: `${HOME}/Library/Developer/Xcode/Archives/2025-11-02`,
    since: "2026-10-01T09:00:00Z",
    reclaim_bytes: "1600000000",
    chosen_by: "policy",
  },
  {
    operation: "0190f0d2-6c1e-7a3b-8c4d-5e6f708192a4",
    state: "retained",
    original: `${HOME}/Library/Caches/com.figma.Desktop`,
    since: "2026-10-14T18:30:00Z",
    reclaim_bytes: "540000000",
    chosen_by: "policy",
  },
];

/** Entries read and folders unreadable so far, as a scan reports them. */
const PROGRESS: readonly (readonly [string, string, string])[] = [
  ["310", "0", `${HOME}/Library/Developer/Xcode/DerivedData`],
  ["940", "4", `${HOME}/Library/Caches/Google/Chrome`],
  ["1580", "12", `${HOME}/Library/Containers`],
];

function wait(ms: number): Promise<void> {
  return ms > 0 ? new Promise((resolve) => setTimeout(resolve, ms)) : Promise.resolve();
}

// The sample uninstall previews (ADR-0062): Spotify may go to Quarantine once
// it is not running; Calculator is part of macOS, and OldEditor has a
// background item.
function uninstallPreviewOf(key: string, n: number, running: boolean): UninstallPreviewView {
  const row = softwareList(false).items.find((item) => item.key === key);
  if (row === undefined || row.origin !== "mac_app") {
    return { kind: "not_found" };
  }
  const app: UninstallAppView = {
    key,
    name: row.name,
    bundle_id: row.id,
    version: row.version,
    path: row.location ?? "",
    size_bytes: "412000000",
    size_complete: true,
    notes: ["goes_to_quarantine", "library_data_stays"],
  };
  if (row.protected) {
    return { kind: "refused", app, reasons: ["not_in_applications", "protected"] };
  }
  if (row.services.length > 0) {
    return { kind: "refused", app, reasons: ["service_linked"] };
  }
  if (running) {
    return { kind: "refused", app, reasons: ["running"] };
  }
  return {
    kind: "ready",
    preview: `0190f0d2-6c1e-7a3b-8c4d-5e6f7081b0${n.toString().padStart(2, "0")}`,
    hash: `b3:${n.toString(16).padStart(64, "0")}`,
    created_at: new Date().toISOString(),
    app,
  };
}

/** A held item's operation ID, made up in the same shape as the core's. */
function operationId(n: number): string {
  return `0190f0d2-6c1e-7a3b-8c4d-5e6f7081a0${n.toString().padStart(2, "0")}`;
}

/**
 * A transport that answers from fixed data, at once or (with `pace`, in
 * milliseconds) with pauses that let a person see each step. Quarantine is
 * kept in memory: cleaning up adds to it, restoring and removing take away.
 * With `builtinKey`, it acts like a release build carrying Lumen's built-in
 * Jeb key (ADR-0042).
 */
export function memoryTransport({
  pace = 0,
  builtinKey = false,
}: {
  readonly pace?: number;
  readonly builtinKey?: boolean;
} = {}): LumenTransport {
  // AI review, as the host keeps it: not chosen and off at first (ADR-0050),
  // and without a key unless the build has one. The key itself is not kept,
  // only that one was given.
  let ai: AiSettingsView = builtinKey
    ? { chosen: false, enabled: false, consented_at: null, key: "jeb", key_source: "builtin" }
    : { chosen: false, enabled: false, consented_at: null, key: "none", key_source: null };
  let held: readonly HeldItemView[] = initiallyHeld;
  let moves = 0;
  let comparing = { stopped: false };
  let previewed = 0;
  // Previews that may still be confirmed, each once.
  const previews = new Map<string, { hash: string; app: UninstallAppView }>();
  // Lumen's helper (ADR-0063), not set up at first.
  let helper: HelperView = { registration: "not_registered", reach: { kind: "not_asked" } };
  // Apps running in the sample, until asked to quit (roadmap 16.3a).
  const running = new Set(["mac_app:com.spotify.client"]);
  const take = (operation: string): HeldItemView | undefined => {
    const item = held.find((h) => h.operation === operation);
    held = held.filter((h) => h.operation !== operation);
    return item;
  };
  return {
    scan: async (_request, onProgress) => {
      // Paced, the scan reports a few growing totals, as a real one does.
      const steps = pace > 0 ? PROGRESS : PROGRESS.slice(-1);
      for (const [entries, unreadable, current] of steps) {
        onProgress?.({
          entries,
          directories: "120",
          allocated_bytes: (BigInt(entries) * 13_000_000n).toString(),
          unreadable,
          current,
          elapsed_ms: "250",
        });
        await wait(pace);
      }
      return scanView;
    },
    assess: async (onProgress) => {
      const result: CandidateView[] = [];
      const total = candidates.length.toString();
      onProgress?.({ stage: "deciding", completed: "0", total });
      for (const candidate of candidates) {
        await wait(pace / Math.max(candidates.length, 1));
        // Like the host: on but without a key, nothing is asked.
        result.push(
          ai.enabled && ai.key !== "none"
            ? { ...candidate, ai_note: notes.get(candidate.path) ?? null }
            : candidate,
        );
        onProgress?.({ stage: "deciding", completed: result.length.toString(), total });
      }
      return result;
    },
    plan: async () => {
      await wait(pace / 2);
      return planView;
    },
    execute: async (request) => {
      await wait(pace);
      const items: ItemReportView[] = planView.items.map((item) => {
        if (item.path === CHANGED) {
          return {
            path: item.path,
            reclaimable_bytes: item.reclaimable_bytes,
            result: { kind: "skipped", reason: "changed" },
          };
        }
        moves += 1;
        const operation = operationId(moves);
        held = [
          ...held,
          {
            operation,
            state: "retained",
            original: item.path,
            since: new Date().toISOString(),
            reclaim_bytes: item.reclaimable_bytes,
            chosen_by: "policy",
          },
        ];
        return {
          path: item.path,
          reclaimable_bytes: item.reclaimable_bytes,
          result: { kind: "quarantined", operation },
        };
      });
      const moved = items.filter((item) => item.result.kind === "quarantined");
      return {
        plan: request.plan,
        resolved: "0",
        items,
        moved_bytes: sumBytes(moved.map((item) => item.reclaimable_bytes)),
        warnings: [],
      };
    },
    held: () => Promise.resolve(held),
    restore: async (operation) => {
      await wait(pace);
      const item = take(operation);
      return Promise.resolve({
        operation,
        original: item?.original ?? null,
        outcome:
          item === undefined
            ? { kind: "not_restorable" }
            : { kind: "restored", parent_replaced: false, verified: true },
      });
    },
    finalize: async (operation) => {
      await wait(pace);
      const item = take(operation);
      return Promise.resolve({
        operation,
        original: item?.original ?? null,
        outcome: item === undefined ? { kind: "not_found" } : { kind: "finalized" },
        space_change: null,
      });
    },
    aiSettings: () => Promise.resolve(ai),
    disk: () => Promise.resolve({ total_bytes: "494384795648", available_bytes: "15032385536" }),
    setAiReview: async (enabled) => {
      await wait(pace);
      if (enabled && ai.key === "none") {
        return Promise.reject(new Error("AI review needs an API key: add your Jeb key first"));
      }
      ai = {
        ...ai,
        chosen: true,
        enabled,
        consented_at: enabled ? new Date().toISOString() : ai.consented_at,
      };
      return Promise.resolve(ai);
    },
    runToolCommand: async () => {
      await wait(pace * 2);
      return {
        success: true,
        exit_code: 0,
        timed_out: false,
        output: "Removed 1,284 packages\n",
        space_changes: null,
      };
    },
    software: async () => {
      await wait(pace * 2);
      return softwareList(false);
    },
    explainSoftware: async () => {
      await wait(pace * 4);
      return softwareList(true);
    },
    toolStorage: async () => {
      await wait(pace * 2);
      const location = (name: string) => ({
        name,
        detail: null,
        bytes: null,
        reported_size: null,
        reported_reclaimable: null,
      });
      return [
        {
          tool: "xcode",
          state: { kind: "found" },
          version: null,
          scope: null,
          items: [
            {
              name: "26.1 (23B86)",
              detail: "Ready",
              bytes: "8724152320",
              reported_size: null,
              reported_reclaimable: null,
            },
          ],
          command: null,
          shared_store: false,
        },
        {
          tool: "docker",
          state: { kind: "unavailable", reason: "the inventory command failed (exit status 1)" },
          version: null,
          scope: null,
          items: [],
          command: null,
          shared_store: false,
        },
        {
          tool: "homebrew",
          state: { kind: "found" },
          version: "Homebrew 7.0.12",
          scope: null,
          items: [location("/Users/demo/Library/Caches/Homebrew")],
          command: "brew cleanup --prune=all --scrub",
          shared_store: false,
        },
        {
          tool: "npm",
          state: { kind: "not_installed" },
          version: null,
          scope: null,
          items: [],
          command: "npm cache clean --force",
          shared_store: false,
        },
        {
          tool: "pnpm",
          state: { kind: "found" },
          version: "10.33.0",
          scope: null,
          items: [location("/Users/demo/Library/pnpm/store/v10")],
          command: "pnpm store prune",
          shared_store: true,
        },
      ];
    },
    compareFolders: async (onProgress) => {
      const run = { stopped: false };
      comparing = run;
      for (const [stage, read] of [
        ["head", "120"],
        ["tail", "18"],
        ["full", "9"],
      ] as const) {
        await wait(pace);
        if (run.stopped) return { kind: "cancelled" };
        onProgress?.({ stage, files_read: read });
      }
      return {
        kind: "compared",
        report: {
          folders: ["/Users/demo/Downloads", "/Users/demo/Pictures"],
          groups: [
            {
              bytes_each: "734003200",
              paths: [
                "/Users/demo/Downloads/Ubuntu 26.04.iso",
                "/Users/demo/Downloads/old/Ubuntu 26.04.iso",
              ],
            },
            {
              bytes_each: "4194304",
              paths: [
                "/Users/demo/Pictures/Trip/IMG_2041.HEIC",
                "/Users/demo/Pictures/Trip copy/IMG_2041.HEIC",
                "/Users/demo/Downloads/IMG_2041.HEIC",
              ],
            },
          ],
          hard_links: [["/Users/demo/Pictures/cover.jpg", "/Users/demo/Pictures/Album/cover.jpg"]],
          skipped: "2",
          protected: "0",
          incomplete: false,
        },
      };
    },
    cancelComparison: async () => {
      comparing.stopped = true;
      await wait(0);
    },
    uninstallPreview: async (key) => {
      await wait(pace);
      previewed += 1;
      const preview = uninstallPreviewOf(key, previewed, running.has(key));
      if (preview.kind === "ready") {
        previews.set(preview.preview, { hash: preview.hash, app: preview.app });
      }
      return preview;
    },
    helperState: async () => {
      await wait(pace);
      return helper;
    },
    helperEnable: async () => {
      await wait(pace);
      helper = { registration: "requires_approval", reach: { kind: "not_asked" } };
      return helper;
    },
    helperDisable: async () => {
      await wait(pace);
      helper = { registration: "not_registered", reach: { kind: "not_asked" } };
      return helper;
    },
    // In the sample, opening Login Items stands for an administrator allowing
    // the helper there.
    helperOpenLoginItems: async () => {
      await wait(pace);
      if (helper.registration === "requires_approval") {
        helper = {
          registration: "enabled",
          reach: { kind: "answered", version: "0.10.0", current: true },
        };
      }
    },
    quitApp: async (key) => {
      await wait(pace);
      const row = softwareList(false).items.find((item) => item.key === key);
      if (row?.origin !== "mac_app") return { kind: "not_found" };
      return { kind: running.delete(key) ? "asked" : "not_running" };
    },
    uninstall: async (preview, confirmation) => {
      await wait(pace);
      const ready = previews.get(preview);
      if (ready === undefined) {
        return Promise.reject(
          new Error(`plan ${preview} was already confirmed or executed; make a new plan`),
        );
      }
      if (ready.hash !== confirmation) {
        return Promise.reject(
          new Error(`the confirmation does not match plan ${preview}; nothing was moved`),
        );
      }
      previews.delete(preview);
      moves += 1;
      const operation = operationId(moves);
      held = [
        ...held,
        {
          operation,
          state: "retained",
          original: ready.app.path,
          since: new Date().toISOString(),
          reclaim_bytes: ready.app.size_bytes,
          chosen_by: "person",
        },
      ];
      return {
        preview,
        path: ready.app.path,
        size_bytes: ready.app.size_bytes,
        resolved: "0",
        result: { kind: "quarantined", operation },
        warnings: [],
      };
    },
    reassess: async ({ ids }) => {
      await wait(pace);
      const chosen =
        ids.length === 0 ? candidates.filter((c) => (c.rechecks ?? []).length > 0) : [];
      for (const id of ids) {
        const found = candidates.filter((c) => c.id === id);
        if (found.length === 0) {
          return Promise.reject(
            new Error("that item is not in the latest assessment; assess again"),
          );
        }
        chosen.push(...found);
      }
      return {
        cancelled: false,
        items: chosen.map((candidate) => {
          const gaps = candidate.rechecks ?? [];
          if (gaps.length === 0) {
            return {
              id: candidate.id,
              before: candidate.verdict,
              gaps,
              outcome: { kind: "settled" },
            };
          }
          // The demo's Discord has been closed since the assessment.
          const now: CandidateView = {
            ...candidate,
            verdict: "quarantine",
            reasons: [
              {
                rule: "classify.app-cache",
                text: "Quarantine proposed: it is an application's cache, which the application re-creates.",
              },
            ],
            free_when_closed: null,
            rechecks: [],
            facts: [devkb("discord-chromium-caches"), ...scanned()],
          };
          return {
            id: candidate.id,
            before: candidate.verdict,
            gaps,
            outcome: { kind: "decided", candidate: now, added_facts: "2" },
          };
        }),
      };
    },
    setAiKey: async (key) => {
      await wait(pace);
      if (!isApiKey(key)) {
        return Promise.reject(
          new Error("that is not an API key (1-256 visible ASCII characters); nothing was stored"),
        );
      }
      ai = { ...ai, key: "jeb", key_source: "stored" };
      return Promise.resolve(ai);
    },
  };
}
