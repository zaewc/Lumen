// An in-memory LumenTransport for development and tests: fixed, plausible
// data that the tests validate with the same schemas as real responses. It
// never touches a filesystem.
import type { CandidateView, HeldItemView, PlanView, ScanView } from "@lumen/api-client";
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
};

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
};

/** A transport that answers from fixed data. */
export function memoryTransport(): LumenTransport {
  return {
    scan: (_request, onProgress) => {
      onProgress?.({
        entries: "1580",
        directories: "120",
        allocated_bytes: "4200000000",
        unreadable: "0",
        current: "/Users/ana/Library/Developer/Xcode/DerivedData",
        elapsed_ms: "250",
      });
      return Promise.resolve(scanView);
    },
    assess: () => Promise.resolve([candidate]),
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
  };
}
