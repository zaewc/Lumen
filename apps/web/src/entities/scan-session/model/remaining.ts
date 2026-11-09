import type { CandidateView, QuarantineReportView } from "@lumen/api-client";

/**
 * The candidates still where the scan found them after a clean-up: every one
 * except those the report says are now in quarantine, and anything inside
 * them. Items that were skipped or failed stay, as assessed.
 */
export function remainingAfter(
  candidates: readonly CandidateView[],
  report: QuarantineReportView,
): readonly CandidateView[] {
  const moved = report.items
    .filter((item) => item.result.kind === "quarantined")
    .map((item) => item.path);
  if (moved.length === 0) {
    return candidates;
  }
  const gone = new Set(moved);
  const inside = (path: string) =>
    moved.some((top) => path.startsWith(`${top}/`) || path.startsWith(`${top}\\`));
  return candidates.filter((candidate) => !gone.has(candidate.path) && !inside(candidate.path));
}
