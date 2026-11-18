import type { CandidateView, ScanView, SoftwareItemView } from "@lumen/api-client";
import { useQuery } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { useMemo, useState } from "react";
import { MoveFlow, MoveResult, softwareTitle } from "@/entities/app-move";
import { type CandidateSummary, DecisionMap, useScanSession } from "@/entities/scan-session";
import { useTransport } from "@/shared/api";
import { useI18n } from "@/shared/i18n";
import { formatBytes, userAgent } from "@/shared/lib";
import { buttonClass, ErrorNotice, InfoTip, Notice } from "@/shared/ui";
import { closingSummary } from "../model/closing";
import { coverageOf } from "../model/coverage";
import { type Filter, shows } from "../model/filter";
import { lensOf } from "../model/lens";
import { listedOwners, ownersOf } from "../model/owners";
import { candidatesIn, topographyOf, trailTo, unreadEntries } from "../model/topography";
import { CandidateList, type Check } from "./CandidateList";
import { CandidateSheet } from "./CandidateSheet";
import { FlowStage } from "./FlowStage";
import { useReviewMove } from "./ReviewMove";
import { ScanDetails } from "./ScanDetails";
import { StepHeading } from "./StepHeading";
import { Topography } from "./Topography";

/** What happened to the apps the last clean-up moved with it (ADR-0069). */
function AppOutcomes() {
  const { m } = useI18n();
  const outcomes = useScanSession().apps;
  if (outcomes === null || outcomes.length === 0) return null;
  const moved = outcomes.filter(
    (app) => app.outcome !== null && app.outcome.result.kind === "quarantined",
  ).length;
  const notMoved = outcomes.length - moved;
  return (
    <Notice tone={notMoved > 0 ? "warning" : "success"}>
      <p>
        {moved > 0 && m.results.apps.moved(moved)}{" "}
        {notMoved > 0 && m.results.apps.notMoved(notMoved)}
      </p>
      {outcomes.some((app) => app.error !== null) && (
        <ul className="mt-1 list-disc pl-5 text-[12px] text-ink-2">
          {outcomes.flatMap((app) =>
            app.error === null ? [] : [<li key={app.preview}>{app.error}</li>],
          )}
        </ul>
      )}
    </Notice>
  );
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
 * a sheet. After a clean-up (`cleanedUp`) it shows what is left of the scan
 * below the report: nothing more can be moved from it, since the core plans
 * from the scan as it was, so it says to scan again instead.
 */
export function Results({
  scan,
  candidates,
  summary,
  filter,
  onFilterChange,
  focus,
  cleanedUp = false,
  check,
}: {
  readonly scan: ScanView;
  readonly candidates: readonly CandidateView[];
  readonly summary: CandidateSummary;
  readonly filter: Filter;
  readonly onFilterChange: (filter: Filter) => void;
  readonly focus: boolean;
  /** A clean-up of this scan ran; its report is shown above. */
  readonly cleanedUp?: boolean;
  /** The checkboxes; after a clean-up the results are read-only. */
  readonly check: Check;
}) {
  const { m } = useI18n();
  const [open, setOpen] = useState<CandidateView | null>(null);
  const move = useReviewMove();
  const proposed = summary.quarantine;
  const groups = useMemo(() => topographyOf(candidates), [candidates]);
  // The group the map is zoomed into, if it is still in this assessment.
  const [zoom, setZoom] = useState<string | null>(null);
  const zoomed = zoom === null ? undefined : trailTo(groups, zoom)?.at(-1);
  const listed = zoomed === undefined ? candidates : candidatesIn(zoomed);
  const transport = useTransport();
  const ask = transport.software;
  const software = useQuery({
    queryKey: ["software"],
    queryFn: () => (ask === undefined ? Promise.reject(new Error("unsupported")) : ask()),
    enabled: ask !== undefined,
  });
  const installed = software.data?.items;
  const [app, setApp] = useState<SoftwareItemView | null>(null);
  // Apps have no verdict: only under "All" and the whole map (ADR-0069). With
  // no items the filter is not shown, so a filter left from before cannot hide them.
  const withApps = (filter === "all" || candidates.length === 0) && zoomed === undefined;
  const { owners, unmeasured } = useMemo(
    () =>
      listedOwners(
        ownersOf(
          listed.filter((candidate) => shows(filter, candidate)),
          installed ?? [],
        ),
        withApps,
      ),
    [listed, filter, installed, withApps],
  );
  const appName = (key: string) => {
    const item = installed?.find((each) => each.key === key);
    return item === undefined ? null : softwareTitle(item);
  };
  const closing = closingSummary(candidates);
  // The open item as assessed now: checking it again replaces it.
  const shown = open === null ? null : (candidates.find((c) => c.path === open.path) ?? open);
  // What the open item is related to, across the whole assessment.
  const lens = useMemo(
    () => (shown === null ? null : lensOf(shown, candidates)),
    [shown, candidates],
  );
  return (
    <>
      {!cleanedUp && <FlowStage step="results" />}
      <div className="space-y-3">
        {move.error}
        {cleanedUp ? (
          <div>
            <h3 className="text-section text-ink tabular-nums">{m.done.left(candidates.length)}</h3>
            <p className="mt-1 text-[13px] text-ink-2">{m.done.scanAgainToMove}</p>
          </div>
        ) : (
          <>
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
          </>
        )}
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
            choice={{
              kind: "filter",
              selected: filter === "all" ? null : filter,
              onSelect: (status) => onFilterChange(status ?? "all"),
              withAll: true,
            }}
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
        </>
      )}
      {software.isError && (
        <ErrorNotice message={m.apps.loadFailed} error={software.error}>
          <button
            type="button"
            className={buttonClass("secondary", "sm")}
            onClick={() => void software.refetch()}
          >
            {m.common.tryAgain}
          </button>
        </ErrorNotice>
      )}
      <MoveResult name={appName} />
      <AppOutcomes />
      {(candidates.length > 0 || owners.length > 0) && (
        <CandidateList
          owners={owners}
          {...(cleanedUp ? {} : { check })}
          onOpen={setOpen}
          onOpenApp={setApp}
          lens={lens}
          moving={move.moving}
        />
      )}
      {withApps && installed !== undefined && (
        <div className="space-y-1 px-2 text-[13px]">
          {unmeasured > 0 && <p className="text-ink-2">{m.results.apps.outside(unmeasured)}</p>}
          <Link to="/apps" className="text-accent-ink hover:underline">
            {m.results.apps.seeAll}
          </Link>
        </div>
      )}
      {app !== null && <MoveFlow item={app} onClose={() => setApp(null)} />}
      <ScanDetails scan={scan} />
      <CandidateSheet
        candidate={shown}
        lens={lens}
        // After a clean-up the results are read-only: scan again to move more.
        {...(cleanedUp
          ? {}
          : {
              move: {
                busy: move.busy,
                onMove: (item: CandidateView) => {
                  setOpen(null);
                  move.start(item);
                },
              },
            })}
        onClose={() => setOpen(null)}
      />
    </>
  );
}
