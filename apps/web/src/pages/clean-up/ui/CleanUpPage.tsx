import type { QuarantineReportView, ScanProgressView } from "@lumen/api-client";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { type ReactNode, useState } from "react";
import { useTransport } from "@/shared/api";
import { useI18n } from "@/shared/i18n";
import { buttonClass, Card } from "@/shared/ui";
import { cleanUpStep } from "../model/step";
import { AiReviewCard } from "./AiReviewCard";
import { DoneCard } from "./DoneCard";
import { Results } from "./Results";
import { ScanningCard } from "./ScanningCard";
import { StartCard } from "./StartCard";
import { StepHeading } from "./StepHeading";

/**
 * The guided clean-up: scan (read-only), let the policy decide, then move
 * what it proposes to quarantine after one explicit confirmation (ADR-0034).
 * Nothing is deleted here; everything moved can be restored.
 */
export function CleanUpPage() {
  const { m } = useI18n();
  const transport = useTransport();
  const queries = useQueryClient();
  const [folders, setFolders] = useState("");
  const [progress, setProgress] = useState<ScanProgressView | null>(null);
  const [report, setReport] = useState<QuarantineReportView | null>(null);
  // Focus follows the flow only once the user has started it.
  const [acted, setActed] = useState(false);
  const scan = useMutation({
    mutationFn: (roots: string[]) => {
      setProgress(null);
      return transport.scan({ roots }, setProgress);
    },
  });
  const scanId = scan.data?.scan;
  // Assessment starts by itself once a scan is complete; it moves nothing.
  const assessment = useQuery({
    queryKey: ["assessment", scanId],
    queryFn: () => transport.assess(),
    enabled: scanId !== undefined,
    retry: false,
    staleTime: Number.POSITIVE_INFINITY,
    refetchOnWindowFocus: false,
  });

  const start = (roots: string[]) => {
    setActed(true);
    setReport(null);
    scan.mutate(roots);
  };
  const startOver = () => {
    setActed(true);
    setReport(null);
    setProgress(null);
    scan.reset();
  };
  const done = (finished: QuarantineReportView) => {
    setActed(true);
    setReport(finished);
    scan.reset();
    queries.removeQueries({ queryKey: ["assessment"] });
    void queries.invalidateQueries({ queryKey: ["held"] });
  };

  const step = cleanUpStep({
    scanning: scan.isPending,
    scanned: scan.data !== undefined,
    assessment: assessment.status,
    done: report !== null,
  });

  const view = (): ReactNode => {
    switch (step) {
      case "start":
        return (
          <>
            <StartCard
              folders={folders}
              onFoldersChange={setFolders}
              onScan={start}
              error={scan.error}
              focus={acted}
            />
            <AiReviewCard />
          </>
        );
      case "scanning":
      case "deciding":
        return <ScanningCard progress={progress} deciding={step === "deciding"} focus={acted} />;
      case "decideFailed":
        return (
          <Card className="space-y-4">
            <StepHeading focus={acted} className="text-xl font-semibold tracking-tight">
              {m.scanning.failed}
            </StepHeading>
            <p className="font-mono text-xs break-all text-neutral-500 dark:text-neutral-400">
              {String(assessment.error)}
            </p>
            <button type="button" onClick={startOver} className={buttonClass("secondary")}>
              {m.common.scanAgain}
            </button>
          </Card>
        );
      case "results":
        return scan.data && assessment.data ? (
          <Results
            scan={scan.data}
            candidates={assessment.data}
            onDone={done}
            onStartOver={startOver}
            focus={acted}
          />
        ) : null;
      case "done":
        return report ? <DoneCard report={report} onStartOver={startOver} focus={acted} /> : null;
    }
  };

  return <main className="mx-auto max-w-3xl space-y-4 px-4 py-8 sm:px-6 sm:py-12">{view()}</main>;
}
