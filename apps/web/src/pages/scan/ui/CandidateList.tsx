import type { CandidateView, VerdictView } from "@lumen/api-client";
import { formatBytes } from "@/shared/lib";

function verdictLabel(verdict: VerdictView): string {
  switch (verdict) {
    case "quarantine":
      return "Proposed for quarantine";
    case "review":
      return "Needs your review";
    case "keep":
      return "Kept";
  }
}

const ORDER: readonly VerdictView[] = ["quarantine", "review", "keep"];

/** Candidates grouped by verdict, each with what it would free and why. */
export function CandidateList({ candidates }: { readonly candidates: readonly CandidateView[] }) {
  if (candidates.length === 0) {
    return <p className="text-sm">No candidates were found in the latest scan.</p>;
  }
  return (
    <div className="space-y-6">
      {ORDER.map((verdict) => {
        const group = candidates.filter((c) => c.verdict === verdict);
        if (group.length === 0) {
          return null;
        }
        return (
          <section key={verdict}>
            <h3 className="font-medium">
              {verdictLabel(verdict)} ({group.length})
            </h3>
            <ul className="mt-2 space-y-3">
              {group.map((candidate) => (
                <li key={candidate.path} className="text-sm">
                  <div className="flex justify-between gap-4">
                    <span className="font-mono break-all">{candidate.path}</span>
                    <span className="shrink-0">{formatBytes(candidate.reclaimable_bytes)}</span>
                  </div>
                  <ul className="mt-1 list-disc pl-5 text-neutral-600">
                    {candidate.reasons.map((reason) => (
                      <li key={reason}>{reason}</li>
                    ))}
                  </ul>
                </li>
              ))}
            </ul>
          </section>
        );
      })}
    </div>
  );
}
