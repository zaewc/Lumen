import type { CandidateView, QuarantineReportView, ScanView } from "@lumen/api-client";
import { useId } from "react";
import { useI18n } from "@/shared/i18n";
import { formatBytes } from "@/shared/lib";
import { buttonClass, Card, InfoTip, Notice } from "@/shared/ui";
import { coverageOf } from "../model/coverage";
import { summarize } from "../model/summary";
import { CandidateSection } from "./CandidateSection";
import { CleanUpAction } from "./CleanUpAction";
import { ScanDetails } from "./ScanDetails";
import { StepHeading } from "./StepHeading";

/**
 * What a scan found: first how much can be freed and the one button that
 * does it, then the items by verdict, then the details.
 */
export function Results({
  scan,
  candidates,
  onDone,
  onStartOver,
  focus,
}: {
  readonly scan: ScanView;
  readonly candidates: readonly CandidateView[];
  readonly onDone: (report: QuarantineReportView) => void;
  readonly onStartOver: () => void;
  readonly focus: boolean;
}) {
  const { m } = useI18n();
  const found = useId();
  const summary = summarize(candidates);
  const proposed = summary.quarantine;
  const coverage = coverageOf(
    scan.roots,
    typeof navigator === "undefined" ? "" : navigator.userAgent,
  );
  return (
    <>
      <Card className="sm:p-8">
        <div className="flex items-start justify-between gap-3">
          <StepHeading focus={focus}>
            <span className="block text-sm font-medium text-neutral-500 dark:text-neutral-400">
              {m.results.canFree}
            </span>
            <span className="mt-1 block text-4xl font-semibold tracking-tight tabular-nums sm:text-5xl">
              {formatBytes(proposed.bytes)}
            </span>
          </StepHeading>
          <InfoTip term="reclaimable" />
        </div>
        {proposed.items.length > 0 && (
          <p className="mt-2 text-neutral-600 dark:text-neutral-400">
            {m.results.proposedCount(proposed.items.length)}
          </p>
        )}
        <div className="mt-6">
          <CleanUpAction proposed={proposed.items.length} onDone={onDone} />
        </div>
      </Card>
      {scan.partial && (
        <Notice tone="warning">
          <p>{scan.out_of_space ? m.results.outOfSpace : m.results.partial}</p>
        </Notice>
      )}
      {coverage.incomplete && (
        <Notice tone="warning">
          <div className="flex items-start gap-2">
            <p className="flex-1">{m.coverage.incomplete}</p>
            <InfoTip term="coverage" />
          </div>
          {coverage.macPrivacy && <p>{m.coverage.fullDiskAccess}</p>}
        </Notice>
      )}
      {candidates.length > 0 && (
        <section aria-labelledby={found} className="space-y-3">
          <div className="flex items-center gap-2 px-1">
            <h2 id={found} className="text-lg font-semibold tracking-tight">
              {m.results.found}
            </h2>
            <InfoTip term="verdicts" />
          </div>
          <CandidateSection title={m.results.proposed} group={proposed} defaultOpen />
          <CandidateSection
            title={m.results.review}
            hint={m.results.reviewHint}
            group={summary.review}
          />
          <CandidateSection title={m.results.keep} hint={m.results.keepHint} group={summary.keep} />
        </section>
      )}
      <ScanDetails scan={scan} />
      <div className="flex justify-center">
        <button type="button" onClick={onStartOver} className={buttonClass("quiet")}>
          {m.common.scanAgain}
        </button>
      </div>
    </>
  );
}
