import type { QuarantineReportView } from "@lumen/api-client";
import { type ReactNode, useState } from "react";
import { useScanSession } from "@/entities/scan-session";
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
 * Nothing is deleted here; everything moved can be restored. The scan
 * belongs to the session, so it goes on while another page is open.
 */
export function CleanUpPage() {
  const { m } = useI18n();
  const session = useScanSession();
  const [folders, setFolders] = useState("");
  // Focus follows the flow only once the user has started it.
  const [acted, setActed] = useState(() => session.scanning);

  const start = (roots: string[]) => {
    setActed(true);
    session.start(roots);
  };
  const startOver = () => {
    setActed(true);
    session.startOver();
  };
  const done = (finished: QuarantineReportView) => {
    setActed(true);
    session.finish(finished);
  };

  const step = cleanUpStep({
    scanning: session.scanning,
    scanned: session.scan !== null,
    assessment: session.assessment,
    done: session.report !== null,
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
              error={session.scanError}
              focus={acted}
            />
            <AiReviewCard />
          </>
        );
      case "scanning":
      case "deciding":
        return (
          <ScanningCard progress={session.progress} deciding={step === "deciding"} focus={acted} />
        );
      case "decideFailed":
        return (
          <Card className="space-y-4">
            <StepHeading focus={acted} className="text-xl font-semibold tracking-tight">
              {m.scanning.failed}
            </StepHeading>
            <p className="font-mono text-xs break-all text-neutral-500 dark:text-neutral-400">
              {String(session.assessmentError)}
            </p>
            <button type="button" onClick={startOver} className={buttonClass("secondary")}>
              {m.common.scanAgain}
            </button>
          </Card>
        );
      case "results":
        return session.scan !== null && session.candidates !== null ? (
          <Results
            scan={session.scan}
            candidates={session.candidates}
            onDone={done}
            onStartOver={startOver}
            focus={acted}
          />
        ) : null;
      case "done":
        return session.report ? (
          <DoneCard report={session.report} onStartOver={startOver} focus={acted} />
        ) : null;
    }
  };

  return <main className="mx-auto max-w-3xl space-y-4 px-4 py-8 sm:px-6 sm:py-12">{view()}</main>;
}
