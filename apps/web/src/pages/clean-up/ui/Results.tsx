import type { CandidateView, ScanView } from "@lumen/api-client";
import { Link } from "@tanstack/react-router";
import { useState } from "react";
import { isOn, useAiSettings } from "@/entities/ai-review";
import type { CandidateSummary } from "@/entities/scan-session";
import { useI18n } from "@/shared/i18n";
import { formatBytes, userAgent } from "@/shared/lib";
import { ChevronIcon, InfoTip, Notice } from "@/shared/ui";
import { coverageOf } from "../model/coverage";
import { sectionsOf } from "../model/sections";
import { CandidateList } from "./CandidateList";
import { CandidateSheet } from "./CandidateSheet";
import { ScanDetails } from "./ScanDetails";
import { StepHeading } from "./StepHeading";

/** Whether AI review took part in this assessment, in one line. */
function AiLine() {
  const { m } = useI18n();
  const settings = useAiSettings();
  if (!settings.isSuccess) {
    return null;
  }
  const service = isOn(settings.data);
  return <>{service === null ? m.ai.statusOff : m.ai.statusOn(service)}</>;
}

/** Folders the scan could not read: a short warning and the way to allow access. */
function CoverageNotice({ scan }: { readonly scan: ScanView }) {
  const { m } = useI18n();
  const coverage = coverageOf(scan.roots, userAgent());
  if (!coverage.incomplete) {
    return null;
  }
  return (
    <Notice tone="warning">
      <div className="flex items-start gap-2">
        <p className="flex-1">
          {m.coverage.incomplete}
          {coverage.macPrivacy && ` ${m.coverage.fullDiskAccess}`}
        </p>
        <InfoTip term="coverage" />
      </div>
      {coverage.macPrivacy && (
        <Link
          to="/settings"
          hash="permissions"
          className="inline-flex items-center gap-0.5 font-semibold text-amber-900 underline-offset-2 hover:underline focus-visible:outline-2 focus-visible:outline-accent dark:text-amber-200"
        >
          {m.coverage.howTo}
          <ChevronIcon className="size-3.5" />
        </Link>
      )}
    </Notice>
  );
}

/**
 * What a scan found: how much can be freed, then every item in sections by
 * group. A row opens its details in a sheet.
 */
export function Results({
  scan,
  candidates,
  summary,
  focus,
}: {
  readonly scan: ScanView;
  readonly candidates: readonly CandidateView[];
  readonly summary: CandidateSummary;
  readonly focus: boolean;
}) {
  const { m } = useI18n();
  const [open, setOpen] = useState<CandidateView | null>(null);
  const proposed = summary.quarantine;
  const sections = sectionsOf(candidates);
  return (
    <>
      <div>
        <p className="text-[12px] font-semibold text-ink-3">{m.results.eyebrow}</p>
        <div className="mt-0.5 flex items-center gap-2">
          <StepHeading
            focus={focus}
            className="text-[24px] leading-tight font-bold tracking-tight text-ink tabular-nums"
          >
            {proposed.items.length > 0
              ? m.results.canFree(formatBytes(proposed.bytes))
              : m.results.nothingTitle}
          </StepHeading>
          <InfoTip term="reclaimable" />
        </div>
        <p className="mt-1 text-[13px] text-ink-2">
          {m.results.found(candidates.length)} <AiLine />
        </p>
      </div>
      {scan.partial && (
        <Notice tone="warning">
          <p>{scan.out_of_space ? m.results.outOfSpace : m.results.partial}</p>
        </Notice>
      )}
      <CoverageNotice scan={scan} />
      {candidates.length > 0 && (
        <>
          <div className="flex items-center gap-2 overflow-x-auto">
            <InfoTip term="verdicts" />
          </div>
          <CandidateList sections={sections} onOpen={setOpen} />
        </>
      )}
      <ScanDetails scan={scan} />
      <CandidateSheet candidate={open} onClose={() => setOpen(null)} />
    </>
  );
}
