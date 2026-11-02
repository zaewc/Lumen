import type { CandidateView, ScanView } from "@lumen/api-client";
import { Link } from "@tanstack/react-router";
import { useMemo, useState } from "react";
import { type CandidateSummary, DecisionMap } from "@/entities/scan-session";
import { useI18n } from "@/shared/i18n";
import { formatBytes, userAgent } from "@/shared/lib";
import { InfoTip, Notice } from "@/shared/ui";
import { closingSummary } from "../model/closing";
import { coverageOf } from "../model/coverage";
import { type Filter, shows } from "../model/filter";
import { lensOf } from "../model/lens";
import { sectionsOf } from "../model/sections";
import { candidatesIn, topographyOf, trailTo, unreadEntries } from "../model/topography";
import { CandidateList } from "./CandidateList";
import { CandidateSheet } from "./CandidateSheet";
import { FlowStage } from "./FlowStage";
import { ScanDetails } from "./ScanDetails";
import { StepHeading } from "./StepHeading";
import { Topography } from "./Topography";

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
 * What a scan found: how much can be freed, the legend that is the one
 * status filter, the map, then every item in sections by group (items with
 * their own command apart from those to review). A row opens its details in
 * a sheet.
 */
export function Results({
  scan,
  candidates,
  summary,
  filter,
  onFilterChange,
  focus,
}: {
  readonly scan: ScanView;
  readonly candidates: readonly CandidateView[];
  readonly summary: CandidateSummary;
  readonly filter: Filter;
  readonly onFilterChange: (filter: Filter) => void;
  readonly focus: boolean;
}) {
  const { m } = useI18n();
  const [open, setOpen] = useState<CandidateView | null>(null);
  const proposed = summary.quarantine;
  const groups = useMemo(() => topographyOf(candidates), [candidates]);
  // The group the map is zoomed into, if it is still in this assessment.
  const [zoom, setZoom] = useState<string | null>(null);
  const zoomed = zoom === null ? undefined : trailTo(groups, zoom)?.at(-1);
  const listed = zoomed === undefined ? candidates : candidatesIn(zoomed);
  const sections = sectionsOf(listed.filter((candidate) => shows(filter, candidate)));
  const closing = closingSummary(candidates);
  // What the open item is related to, across the whole assessment.
  const lens = useMemo(() => (open === null ? null : lensOf(open, candidates)), [open, candidates]);
  return (
    <>
      <FlowStage step="results" />
      <div className="space-y-3">
        <div>
          <StepHeading focus={focus} className="text-section text-ink tabular-nums">
            {m.workspace.assessment(candidates.length)}
          </StepHeading>
          <p className="mt-1 text-[13px] text-ink-2">
            {proposed.items.length > 0
              ? m.results.canFree(formatBytes(proposed.bytes))
              : m.results.nothingTitle}
            {proposed.items.length > 0 && ` · ${m.common.items(proposed.items.length)}`}
          </p>
        </div>
        {proposed.items.length === 0 && <p className="text-ink-2">{m.results.nothing}</p>}
        {closing !== null && (
          <p className="text-[13px] text-ink-2">
            {m.results.whenClosed(formatBytes(closing.bytes), ...closing.apps)}
          </p>
        )}
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
            onSelect={(status) => onFilterChange(status ?? "all")}
            withAll
            accessory={<InfoTip term="verdicts" />}
            track={false}
          />
          <Topography
            groups={groups}
            unread={unreadEntries(scan.roots)}
            zoom={zoomed?.id ?? null}
            onZoom={setZoom}
            filter={filter}
            lens={lens}
            onOpen={setOpen}
          />
          <CandidateList sections={sections} onOpen={setOpen} lens={lens} />
        </>
      )}
      <ScanDetails scan={scan} />
      <CandidateSheet candidate={open} lens={lens} onClose={() => setOpen(null)} />
    </>
  );
}
