import type { ScanView } from "@lumen/api-client";
import { useQuery } from "@tanstack/react-query";
import { useState } from "react";
import { CandidateList } from "@/entities/candidate";
import { useTransport } from "@/shared/api";
import { useI18n } from "@/shared/i18n";
import { InfoTip } from "@/shared/ui";
import { RootCard } from "./RootCard";
import { ScanForm } from "./ScanForm";

/** Scan chosen folders, then see what the policy decides about them. */
export function ScanPage() {
  const { m } = useI18n();
  const transport = useTransport();
  const [scan, setScan] = useState<ScanView | null>(null);
  const assessment = useQuery({
    queryKey: ["assessment", scan?.scan],
    queryFn: () => transport.assess(),
    enabled: scan !== null,
  });
  return (
    <main className="mx-auto max-w-3xl space-y-8 p-8">
      <div className="flex items-center gap-2">
        <h1 className="text-2xl font-semibold">{m.scan.title}</h1>
        <InfoTip term="scan" />
      </div>
      <ScanForm onScanned={setScan} />
      {scan && (
        <section className="space-y-3">
          <h2 className="text-lg font-medium">{m.scan.found}</h2>
          {scan.partial && (
            <p className="text-sm text-amber-800">
              {scan.out_of_space ? m.scan.outOfSpace : m.scan.partial}
            </p>
          )}
          {scan.roots.map((root) => (
            <RootCard key={root.root} root={root} />
          ))}
        </section>
      )}
      {scan && (
        <section className="space-y-3">
          <div className="flex items-center gap-2">
            <h2 className="text-lg font-medium">{m.scan.assessment}</h2>
            <InfoTip term="verdicts" />
          </div>
          <p className="text-xs text-neutral-600">{m.scan.nothingMoved}</p>
          {assessment.isPending && <p className="text-sm">{m.scan.deciding}</p>}
          {assessment.isError && <p className="text-sm text-red-700">{String(assessment.error)}</p>}
          {assessment.data && <CandidateList candidates={assessment.data} />}
        </section>
      )}
    </main>
  );
}
