import type { CandidateView, ScanView } from "@lumen/api-client";
import { Link } from "@tanstack/react-router";
import { useState } from "react";
import { type CandidateSummary, DecisionMap } from "@/entities/scan-session";
import { useI18n } from "@/shared/i18n";
import { formatBytes, userAgent } from "@/shared/lib";
import { buttonClass, InfoTip, Notice } from "@/shared/ui";
import { coverageOf } from "../model/coverage";
import { type Filter, shows } from "../model/filter";
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
 * group, filtered by status (items with their own command apart from those
 * to review). A row opens its details in a sheet.
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
  const sections = sectionsOf(candidates.filter((candidate) => shows(filter, candidate)));
  return (
    <>
      <div className="space-y-3">
        <div className="flex flex-wrap items-center justify-between gap-x-4 gap-y-2">
          <div>
            <StepHeading
              focus={focus}
              className="text-[24px] font-medium tracking-[-0.04em] text-ink tabular-nums"
            >
              {m.workspace.assessment(candidates.length)}
            </StepHeading>
            <p className="mt-1 text-[13px] text-ink-2">
              {proposed.items.length > 0
                ? m.results.canFree(formatBytes(proposed.bytes))
                : m.results.nothingTitle}
              {proposed.items.length > 0 && ` · ${m.common.items(proposed.items.length)}`}
            </p>
          </div>
          {candidates.length > 0 && (
            <div className="flex max-w-full flex-wrap items-center gap-2">
              <button
                type="button"
                onClick={() => onFilterChange("all")}
                aria-pressed={filter === "all"}
                className={buttonClass(filter === "all" ? "secondary" : "quiet", "sm")}
              >
                {m.results.filters.all}
              </button>
              <InfoTip term="verdicts" />
            </div>
          )}
        </div>
        {proposed.items.length === 0 && <p className="text-ink-2">{m.results.nothing}</p>}
        {scan.partial && (
          <Notice tone="warning">
            <p>{scan.out_of_space ? m.results.outOfSpace : m.results.partial}</p>
          </Notice>
        )}
        <CoverageNotice scan={scan} />
      </div>
      {candidates.length > 0 && (
        <>
          <DecisionMap
            summary={summary}
            selected={filter === "all" ? null : filter}
            onSelect={onFilterChange}
          />
          <CandidateList sections={sections} onOpen={setOpen} selectedPath={open?.path ?? null} />
        </>
      )}
      <ScanDetails scan={scan} onStartOver={onStartOver} />
      <CandidateSheet candidate={open} onClose={() => setOpen(null)} />
    </>
  );
}
