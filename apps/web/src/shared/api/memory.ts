// An in-memory LumenTransport for development and tests: fixed, plausible
// data that the tests validate with the same schemas as real responses. It
// never touches a filesystem, a keychain or the network.
import type {
  AiSettingsView,
  CandidateView,
  HeldItemView,
  PlanView,
  ScanView,
} from "@lumen/api-client";
import { isApiKey } from "@/shared/lib";
import type { LumenTransport } from "./transport";

const scanView: ScanView = {
  scan: "0190f0d2-6c1e-7a3b-8c4d-5e6f708192a1",
  roots: [
    {
      root: "/Users/ana/Library/Developer",
      files: "1204",
      logical_bytes: "8400000000",
      allocated_bytes: "8500000000",
      reclaimable_bytes: "8100000000",
      reclaimable_exact: true,
      coverage_complete: true,
      denied: "0",
      failed: "0",
      not_scanned: "0",
    },
    {
      root: "/Users/ana/Library/Containers",
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

const candidate: CandidateView = {
  path: "/Users/ana/Library/Developer/Xcode/DerivedData/App-abc",
  verdict: "quarantine",
  reclaimable_bytes: "4200000000",
  reasons: [
    {
      rule: "classify.devkb-regenerable",
      text: "Quarantine proposed: the tool that made it can regenerate it.",
    },
  ],
  tool_command: null,
  group: { kind: "developer", source: "Xcode" },
};

/** Candidates the policy does not propose: one to review, one it keeps. */
const others: readonly CandidateView[] = [
  {
    path: "/Users/ana/Library/Developer/CoreSimulator/Devices",
    verdict: "review",
    reclaimable_bytes: "310000000",
    reasons: [
      {
        rule: "classify.devkb-tool-action",
        text: "Review: only the tool's own cleanup command should remove it, and that cannot be undone.",
      },
    ],
    tool_command: {
      tool: "Xcode",
      program: "xcrun",
      args: ["simctl", "delete", "unavailable"],
    },
    group: { kind: "developer", source: "Xcode" },
  },
  {
    path: "/Users/ana/Library/Mobile Documents/com~apple~CloudDocs",
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
  },
];

const planView: PlanView = {
  plan: "0190f0d2-6c1e-7a3b-8c4d-5e6f708192a2",
  hash: "b3:8c05bf8f00722b4a58174b715d1b11cfc4f69000c54b269df53b85cfa0cff22f",
  created_at: "2026-10-16T09:00:00Z",
  items: [candidate],
};

const held: HeldItemView = {
  operation: "0190f0d2-6c1e-7a3b-8c4d-5e6f708192a3",
  state: "retained",
  original: candidate.path,
  since: "2026-10-01T09:00:00Z",
  reclaim_bytes: candidate.reclaimable_bytes,
};

/** Entries read and folders unreadable so far, as a scan reports them. */
const PROGRESS: readonly (readonly [string, string])[] = [
  ["310", "0"],
  ["940", "4"],
  ["1580", "12"],
];

function wait(ms: number): Promise<void> {
  return ms > 0 ? new Promise((resolve) => setTimeout(resolve, ms)) : Promise.resolve();
}

/**
 * A transport that answers from fixed data, at once or (with `pace`, in
 * milliseconds) with pauses that let a person see each step.
 */
export function memoryTransport({ pace = 0 }: { readonly pace?: number } = {}): LumenTransport {
  // AI review, as the host keeps it: off and without a key at first. The key
  // itself is not kept, only that one was given.
  let ai: AiSettingsView = { enabled: false, consented_at: null, key: "none" };
  return {
    scan: async (_request, onProgress) => {
      // Paced, the scan reports a few growing totals, as a real one does.
      const steps = pace > 0 ? PROGRESS : PROGRESS.slice(-1);
      for (const [entries, unreadable] of steps) {
        onProgress?.({
          entries,
          directories: "120",
          allocated_bytes: "4200000000",
          unreadable,
          current: "/Users/ana/Library/Developer/Xcode/DerivedData",
          elapsed_ms: "250",
        });
        await wait(pace);
      }
      return scanView;
    },
    assess: async () => {
      await wait(pace);
      return [candidate, ...others];
    },
    plan: () => Promise.resolve(planView),
    execute: (request) =>
      Promise.resolve({
        plan: request.plan,
        resolved: "0",
        items: [
          {
            path: candidate.path,
            reclaimable_bytes: candidate.reclaimable_bytes,
            result: { kind: "quarantined", operation: held.operation },
          },
        ],
        moved_bytes: candidate.reclaimable_bytes,
        warnings: [],
      }),
    held: () => Promise.resolve([held]),
    restore: (operation) =>
      Promise.resolve({
        operation,
        original: held.original ?? null,
        outcome: { kind: "restored", parent_replaced: false, verified: true },
      }),
    finalize: (operation) =>
      Promise.resolve({
        operation,
        original: held.original ?? null,
        outcome: { kind: "finalized" },
      }),
    aiSettings: () => Promise.resolve(ai),
    disk: () => Promise.resolve({ total_bytes: "494384795648", available_bytes: "15032385536" }),
    setAiReview: (enabled) => {
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
    setAiKey: (key) => {
      if (!isApiKey(key)) {
        return Promise.reject(
          new Error("that is not an API key (1-256 visible ASCII characters); nothing was stored"),
        );
      }
      ai = { ...ai, key: "jeb" };
      return Promise.resolve(ai);
    },
  };
}
