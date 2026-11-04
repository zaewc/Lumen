import type { CandidateView } from "@lumen/api-client";
import { useI18n } from "@/shared/i18n";
import { formatBytes } from "@/shared/lib";
import { Bytes } from "@/shared/ui";

/**
 * What removing a candidate frees. A figure the scan could only bound from
 * below (shared blocks not ruled out, ADR-0017) says so, as a scan's totals do.
 */
export function CandidateSize({ candidate }: { readonly candidate: CandidateView }) {
  const { m } = useI18n();
  if (candidate.reclaimable_exact) {
    return <Bytes value={candidate.reclaimable_bytes} />;
  }
  return <>{m.details.atLeast(formatBytes(candidate.reclaimable_bytes))}</>;
}
