import type { CompareOutcomeView, CompareProgressView, DuplicatesView } from "@lumen/api-client";
import { useEffect, useId, useRef, useState } from "react";
import { useSessionOperation, useTransport } from "@/shared/api";
import { useI18n } from "@/shared/i18n";
import { formatBytes } from "@/shared/lib";
import { buttonClass, ErrorNotice } from "@/shared/ui";
import { type CompareNote, compareNotes, SHOWN_GROUPS } from "../model/duplicates";

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
function Report({ report }: { readonly report: DuplicatesView }) {
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
              <ul className="font-mono text-[12px] break-all text-ink-2 select-text">
                {group.paths.map((path) => (
                  <li key={path}>{path}</li>
                ))}
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

function Outcome({ outcome }: { readonly outcome: CompareOutcomeView }) {
  const { m } = useI18n();
  const t = m.overview.copies;
  switch (outcome.kind) {
    case "unsupported":
      return <p className="text-[13px] text-ink-2">{t.unsupported}</p>;
    case "too_many":
      return <p className="text-[13px] text-ink-2">{t.tooMany(outcome.limit)}</p>;
    case "cancelled":
      return <p className="text-[13px] text-ink-2">{t.stopped}</p>;
    case "compared":
      return <Report report={outcome.report} />;
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
export function Duplicates({ scanId }: { readonly scanId: string }) {
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
      {!pending && run.data !== null && <Outcome outcome={run.data} />}
    </section>
  );
}
