import type { CandidateView, ScanView } from "@lumen/api-client";
import { Link } from "@tanstack/react-router";
import { useState } from "react";
import type { CandidateSummary } from "@/entities/scan-session";
import { useI18n } from "@/shared/i18n";
import { formatBytes, userAgent } from "@/shared/lib";
import { InfoTip, Notice, Segmented } from "@/shared/ui";
import { coverageOf } from "../model/coverage";
import { FILTERS, type Filter, filterCounts, shows } from "../model/filter";
import { sectionsOf } from "../model/sections";
import { CandidateList } from "./CandidateList";
import { CandidateSheet } from "./CandidateSheet";
import { ScanDetails } from "./ScanDetails";
import { StepHeading } from "./StepHeading";

/** Folders the scan could not read: a short warning and the way to allow access. */
function CoverageNotice({ scan }: { readonly scan: ScanView }) {
  const { m } = useI18n();
  const coverage = coverageOf(scan.roots, userAgent());
  if (!coverage.incomplete) {
    return null;
  }
  return (
    <Notice tone="warning">
      <p>
        {m.coverage.incomplete}{" "}
        {coverage.macPrivacy && (
          <Link to="/settings" hash="permissions" className="text-accent-ink hover:underline">
            {m.coverage.howTo}
          </Link>
        )}
      </p>
    </Notice>
  );
}

/**
 * What a scan found: how much can be freed, then every item in sections by
 * group, filtered by verdict. A row opens its details in a sheet.
 */
export function Results({
  scan,
  candidates,
  summary,
  filter,
  onFilterChange,
  onStartOver,
  focus,
}: {
  readonly scan: ScanView;
  readonly candidates: readonly CandidateView[];
  readonly summary: CandidateSummary;
  readonly filter: Filter;
  readonly onFilterChange: (filter: Filter) => void;
  /** Back to the start, to scan other folders. */
  readonly onStartOver: () => void;
  readonly focus: boolean;
}) {
  const { m } = useI18n();
  const [open, setOpen] = useState<CandidateView | null>(null);
  const proposed = summary.quarantine;
  const counts = filterCounts(candidates);
  const sections = sectionsOf(candidates.filter((candidate) => shows(filter, candidate)));
  return (
    <>
      <div className="space-y-3">
        <div className="flex flex-wrap items-center justify-between gap-x-4 gap-y-2">
          <StepHeading focus={focus} className="text-[15px] font-semibold text-ink tabular-nums">
            {proposed.items.length > 0 ? (
              <>
                {m.results.canFree(formatBytes(proposed.bytes))}
                <span className="ml-2 text-[13px] font-normal text-ink-2">
                  {m.common.items(proposed.items.length)}
                </span>
              </>
            ) : (
              m.results.nothingTitle
            )}
          </StepHeading>
          {candidates.length > 0 && (
            <div className="flex max-w-full items-center gap-2 overflow-x-auto">
              <Segmented
                label={m.results.filter}
                value={filter}
                onChange={onFilterChange}
                segments={FILTERS.map((value) => ({
                  value,
                  label: m.results.filters[value],
                  count: counts[value],
                }))}
              />
              <InfoTip term="verdicts" />
            </div>
          )}
        </div>
        {scan.partial && (
          <Notice tone="warning">
            <p>{scan.out_of_space ? m.results.outOfSpace : m.results.partial}</p>
          </Notice>
        )}
        <CoverageNotice scan={scan} />
      </div>
      {candidates.length > 0 && <CandidateList sections={sections} onOpen={setOpen} />}
      <ScanDetails scan={scan} onStartOver={onStartOver} />
      <CandidateSheet candidate={open} onClose={() => setOpen(null)} />
    </>
  );
}
