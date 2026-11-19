import type {
  CompareOutcomeView,
  CompareProgressView,
  CopyRequest,
  DuplicatesView,
} from "@lumen/api-client";
import { useEffect, useId, useRef, useState } from "react";
import { useSessionOperation, useTransport } from "@/shared/api";
import { useI18n } from "@/shared/i18n";
import { formatBytes } from "@/shared/lib";
import { buttonClass, ErrorNotice } from "@/shared/ui";
import { type CompareNote, compareNotes, SHOWN_GROUPS } from "../model/duplicates";
import { type CopyCheck, copyKey } from "../model/selection";

function NoteText({ note }: { readonly note: CompareNote }) {
  const { m } = useI18n();
  const t = m.overview.copies;
  switch (note.kind) {
    case "hard_links":
      return t.hardLinks(note.count);
    case "skipped":
      return t.skipped(note.count);
    case "protected":
      return t.protectedFiles(note.count);
    case "incomplete":
      return t.incomplete;
    default: {
      const unknown: never = note;
      return unknown;
    }
  }
}

/** Groups largest first, then what was not compared; never a choice of copy. */
/** The copies' checkboxes (ADR-0069): Lumen never checks one, and one copy of each group stays. */
export interface CopyCheckProps {
  readonly state: (key: string) => CopyCheck | undefined;
  readonly toggle: (key: string, request: CopyRequest) => void;
}

function Copy({
  report,
  group,
  at,
  check,
}: {
  readonly report: DuplicatesView;
  readonly group: number;
  readonly at: number;
  readonly check: CopyCheckProps;
}) {
  const { m } = useI18n();
  const t = m.overview.copies;
  const paths = report.groups[group]?.paths ?? [];
  const path = paths[at] ?? "";
  const key = copyKey(report.comparison, group, at);
  const state = check.state(key)?.check;
  const entryAt = (place: number) => check.state(copyKey(report.comparison, group, place));
  const taken = (place: number) => {
    const other = entryAt(place);
    return other !== undefined && other.check.kind !== "refused";
  };
  const checked = state !== undefined && state.kind !== "refused";
  // One copy always stays: a copy another checked copy keeps, or the last
  // unchecked one, cannot be checked.
  const kept = paths.some(
    (_, place) => place !== at && taken(place) && entryAt(place)?.stays === at,
  );
  const staying = paths.map((_, place) => place).filter((place) => place !== at && !taken(place));
  const last = !checked && (staying.length === 0 || kept);
  const stays = staying[0];
  return (
    <li className="flex items-start gap-2">
      <input
        type="checkbox"
        checked={checked}
        disabled={last || state?.kind === "checking"}
        onChange={() =>
          check.toggle(key, {
            comparison: report.comparison,
            group,
            copy: at,
            // Unchecking ignores it; checking needs one that stays.
            stays: stays ?? at,
          })
        }
        aria-label={m.action.check(path)}
        className="mt-0.5 size-4 shrink-0 cursor-pointer accent-accent disabled:cursor-not-allowed"
      />
      <span className="min-w-0">
        <span className="block">{path}</span>
        {last && <span className="block font-sans text-ink-3">{t.oneStays}</span>}
        {state?.kind === "checking" && (
          <span className="block font-sans text-ink-3">{m.results.apps.checking}</span>
        )}
        {state?.kind === "refused" && (
          <span className="block font-sans text-attention">{t.cannotMove}</span>
        )}
      </span>
    </li>
  );
}

function Report({
  report,
  check,
}: {
  readonly report: DuplicatesView;
  readonly check?: CopyCheckProps;
}) {
  const { m } = useI18n();
  const t = m.overview.copies;
  const shown = report.groups.slice(0, SHOWN_GROUPS);
  const hidden = report.groups.length - shown.length;
  const notes = compareNotes(report);
  return (
    <div className="space-y-3">
      <div className="text-[12px] text-ink-2">
        {t.folders}
        <ul className="font-mono break-all text-ink-3">
          {report.folders.map((folder) => (
            <li key={folder}>{folder}</li>
          ))}
        </ul>
      </div>
      {shown.length === 0 ? (
        <p className="text-[13px] text-ink-2">{t.none}</p>
      ) : (
        <ul className="divide-y divide-line">
          {shown.map((group) => (
            <li key={group.paths.join("\n")} className="space-y-1 py-3">
              <p className="text-[13px] font-semibold text-ink tabular-nums">
                {t.group(group.paths.length, formatBytes(group.bytes_each))}
              </p>
              <ul className="space-y-1 font-mono text-[12px] break-all text-ink-2 select-text">
                {group.paths.map((path, at) =>
                  check === undefined ? (
                    <li key={path}>{path}</li>
                  ) : (
                    <Copy
                      key={path}
                      report={report}
                      group={report.groups.indexOf(group)}
                      at={at}
                      check={check}
                    />
                  ),
                )}
              </ul>
            </li>
          ))}
        </ul>
      )}
      {hidden > 0 && <p className="text-[12px] text-ink-2">{t.more(hidden)}</p>}
      {notes.map((note) => (
        <p key={note.kind} className="text-[12px] text-ink-2">
          <NoteText note={note} />
        </p>
      ))}
      {shown.length > 0 && <p className="text-[12px] text-ink-2">{t.advice}</p>}
    </div>
  );
}

function Outcome({
  outcome,
  check,
}: {
  readonly outcome: CompareOutcomeView;
  readonly check?: CopyCheckProps;
}) {
  const { m } = useI18n();
  const t = m.overview.copies;
  switch (outcome.kind) {
    case "unsupported":
      return <p className="text-[13px] text-ink-2">{t.unsupported}</p>;
    case "cancelled":
      return <p className="text-[13px] text-ink-2">{t.stopped}</p>;
    case "compared":
      return <Report report={outcome.report} {...(check === undefined ? {} : { check })} />;
    default: {
      const unknown: never = outcome;
      return unknown;
    }
  }
}

/**
 * Runs a separate, read-only comparison after a complete scan. The host
 * verifies the scan ID and reads only that scan's roots (ADR-0065).
 */
export function Duplicates({
  scanId,
  check,
  onCompare,
}: {
  readonly scanId: string;
  /** The copies' checkboxes; none when copies cannot be moved here. */
  readonly check?: CopyCheckProps;
  /** A comparison starts: copies checked in the last one no longer count. */
  readonly onCompare?: () => void;
}) {
  const { m } = useI18n();
  const transport = useTransport();
  const compare = transport.compareScan;
  const stop = transport.cancelComparison;
  const title = useId();
  const started = useRef<string | null>(null);
  const [stopping, setStopping] = useState(false);
  const [stopError, setStopError] = useState<unknown>(null);
  const run = useSessionOperation(
    ["session-operation", "duplicate-comparison", scanId],
    async (id: string, report: (progress: CompareProgressView) => void) => {
      if (compare === undefined) throw new Error("Duplicate comparison is unavailable");
      return compare(id, report);
    },
  );
  const pending = run.status === "pending";
  const canCompareAgain =
    run.status === "error" || run.data?.kind === "compared" || run.data?.kind === "cancelled";
  useEffect(() => {
    if (compare !== undefined && run.status === "idle" && started.current !== scanId) {
      started.current = scanId;
      onCompare?.();
      void run.run(scanId);
    }
  }, [compare, run.run, run.status, scanId]);
  useEffect(() => {
    if (!pending) setStopping(false);
  }, [pending]);
  if (compare === undefined) return null;
  const t = m.overview.copies;
  return (
    <section aria-labelledby={title} aria-busy={pending} className="space-y-3 pt-8">
      <div className="flex items-baseline justify-between gap-3">
        <h2 id={title} className="text-section text-ink">
          {t.title}
        </h2>
        <div className="flex gap-2">
          {pending && stop !== undefined && (
            <button
              type="button"
              disabled={stopping}
              onClick={() => {
                setStopping(true);
                setStopError(null);
                void stop().catch((error: unknown) => {
                  setStopError(error);
                  setStopping(false);
                });
              }}
              className={buttonClass("secondary", "sm")}
            >
              {stopping ? t.stopping : t.stop}
            </button>
          )}
          {!pending && canCompareAgain && (
            <button
              type="button"
              onClick={() => {
                started.current = scanId;
                setStopError(null);
                onCompare?.();
                void run.run(scanId);
              }}
              className={buttonClass("secondary", "sm")}
            >
              {t.compareAgain}
            </button>
          )}
        </div>
      </div>
      {run.data === null && !pending && run.status !== "error" && (
        <p className="text-[13px] text-ink-2">{t.intro}</p>
      )}
      {run.progress !== null && (
        <p role="status" className="text-[13px] text-ink-2 tabular-nums">
          {t.stage[run.progress.stage]} · {t.filesRead(run.progress.files_read)}
        </p>
      )}
      {stopError !== null && <ErrorNotice message={t.failed} error={stopError} />}
      {run.status === "error" && <ErrorNotice message={t.failed} error={run.error} />}
      {!pending && run.data !== null && (
        <Outcome outcome={run.data} {...(check === undefined ? {} : { check })} />
      )}
    </section>
  );
}
