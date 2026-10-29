// An in-memory LumenTransport for development and tests: fixed, plausible
// data that the tests validate with the same schemas as real responses. It
// never touches a filesystem, a keychain or the network.
import type {
  AiSettingsView,
  CandidateView,
  HeldItemView,
  ItemReportView,
  PlanView,
  ScanView,
} from "@lumen/api-client";
import { isApiKey, sumBytes } from "@/shared/lib";
import type { LumenTransport } from "./transport";

const HOME = "/Users/ana";

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

/**
 * What a scan of the fixture finds: every group, every verdict, and items to
 * clean up with their tool's own command (ADR-0039).
 */
const candidates: readonly CandidateView[] = [
  {
    path: `${HOME}/Library/Developer/Xcode/DerivedData`,
    verdict: "quarantine",
    reclaimable_bytes: "4200000000",
    reasons: [regenerable],
    tool_command: null,
    group: { kind: "developer", source: "Xcode" },
    ai_note: null,
  },
  {
    path: `${HOME}/Library/Developer/CoreSimulator/Devices`,
    verdict: "review",
    reclaimable_bytes: "3100000000",
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
  },
  {
    path: `${HOME}/Library/pnpm/store`,
    verdict: "review",
    reclaimable_bytes: "5200000000",
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
  },
  {
    path: `${HOME}/.gradle/caches`,
    verdict: "quarantine",
    reclaimable_bytes: "2600000000",
    reasons: [regenerable],
    tool_command: null,
    group: { kind: "developer", source: "Gradle" },
    ai_note: null,
  },
  {
    path: `${HOME}/.npm/_cacache`,
    verdict: "quarantine",
    reclaimable_bytes: "1800000000",
    reasons: [regenerable],
    tool_command: null,
    group: { kind: "developer", source: "npm" },
    ai_note: null,
  },
  {
    path: `${HOME}/Library/Caches/go-build`,
    verdict: "keep",
    reclaimable_bytes: "890000000",
    reasons: [
      {
        rule: "classify.devkb-self-managed",
        text: "Kept: the tool that made it cleans it up automatically.",
      },
    ],
    tool_command: null,
    group: { kind: "developer", source: "Go" },
    ai_note: null,
  },
  {
    path: `${HOME}/Library/Caches/com.spotify.client`,
    verdict: "quarantine",
    reclaimable_bytes: "1300000000",
    reasons: [
      {
        rule: "classify.app-cache",
        text: "Quarantine proposed: it is an application's cache, which the application re-creates.",
      },
    ],
    tool_command: null,
    group: { kind: "app_cache", source: "com.spotify.client" },
    ai_note: null,
  },
  {
    path: `${HOME}/Library/Caches/Google/Chrome`,
    verdict: "review",
    reclaimable_bytes: "760000000",
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
  },
  {
    path: `${HOME}/Library/Application Support/Slack`,
    verdict: "keep",
    reclaimable_bytes: "980000000",
    reasons: [
      {
        rule: "classify.app-state",
        text: "Kept: it holds an application's own data, settings or documents; Lumen leaves it alone.",
      },
    ],
    tool_command: null,
    group: { kind: "app_data", source: "com.tinyspeck.slackmacgap" },
    ai_note: null,
  },
  {
    path: `${HOME}/Library/Application Support/OldEditor`,
    verdict: "review",
    reclaimable_bytes: "420000000",
    reasons: [
      {
        rule: "classify.orphan",
        text: "Review: the application it belongs to appears to be uninstalled.",
      },
    ],
    tool_command: null,
    group: { kind: "app_data", source: "com.example.OldEditor" },
    ai_note: null,
  },
  {
    path: `${HOME}/Library/HTTPStorages/com.tinyspeck.slackmacgap`,
    verdict: "keep",
    reclaimable_bytes: "380928",
    reasons: [{ rule: "default.keep-small", text: "Kept: too small to be worth reviewing." }],
    tool_command: null,
    group: { kind: "other", source: null },
    ai_note: null,
  },
  {
    path: `${HOME}/Downloads/Installers`,
    verdict: "review",
    reclaimable_bytes: "2300000000",
    reasons: [{ rule: "default.review", text: "Review: Lumen does not recognise this item." }],
    tool_command: null,
    group: { kind: "other", source: null },
    ai_note: null,
  },
  {
    path: `${HOME}/Library/Mobile Documents/com~apple~CloudDocs`,
    verdict: "keep",
    reclaimable_bytes: "0",
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
  },
  {
    operation: "0190f0d2-6c1e-7a3b-8c4d-5e6f708192a4",
    state: "retained",
    original: `${HOME}/Library/Caches/com.figma.Desktop`,
    since: "2026-10-14T18:30:00Z",
    reclaim_bytes: "540000000",
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
  // AI review, as the host keeps it: on at first (ADR-0047), and without a
  // key unless the build has one. The key itself is not kept, only that one
  // was given.
  let ai: AiSettingsView = builtinKey
    ? { enabled: true, consented_at: null, key: "jeb", key_source: "builtin" }
    : { enabled: true, consented_at: null, key: "none", key_source: null };
  let held: readonly HeldItemView[] = initiallyHeld;
  let moves = 0;
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
        enabled,
        consented_at: enabled ? new Date().toISOString() : ai.consented_at,
      };
      return Promise.resolve(ai);
    },
    runToolCommand: async () => {
      await wait(pace * 2);
      return { success: true, exit_code: 0, timed_out: false, output: "Removed 1,284 packages\n" };
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
