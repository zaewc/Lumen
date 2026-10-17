import type { ScanView } from "@lumen/api-client";
import { useQuery } from "@tanstack/react-query";
import { useState } from "react";
import { useTransport } from "@/shared/api";
import { CandidateList } from "./CandidateList";
import { RootCard } from "./RootCard";
import { ScanForm } from "./ScanForm";

/** Scan chosen folders, then see what the policy decides about them. */
export function ScanPage() {
  const transport = useTransport();
  const [scan, setScan] = useState<ScanView | null>(null);
  const assessment = useQuery({
    queryKey: ["assessment", scan?.scan],
    queryFn: () => transport.assess(),
    enabled: scan !== null,
  });
  return (
    <main className="mx-auto max-w-3xl space-y-8 p-8">
      <h1 className="text-2xl font-semibold">Scan</h1>
      <ScanForm onScanned={setScan} />
      {scan && (
        <section className="space-y-3">
          <h2 className="text-lg font-medium">What was found</h2>
          {scan.partial && (
            <p className="text-sm text-amber-800">The scan stopped early; results are partial.</p>
          )}
          {scan.roots.map((root) => (
            <RootCard key={root.root} root={root} />
          ))}
        </section>
      )}
      {scan && (
        <section className="space-y-3">
          <h2 className="text-lg font-medium">Assessment</h2>
          <p className="text-xs text-neutral-600">Nothing has been moved.</p>
          {assessment.isPending && <p className="text-sm">Deciding…</p>}
          {assessment.isError && <p className="text-sm text-red-700">{String(assessment.error)}</p>}
          {assessment.data && <CandidateList candidates={assessment.data} />}
        </section>
      )}
    </main>
  );
}
