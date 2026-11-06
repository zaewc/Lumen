import type { CompareOutcomeView, CompareProgressView, DuplicatesView } from "@lumen/api-client";
import { useMutation } from "@tanstack/react-query";
import { useId, useState } from "react";
import { useTransport } from "@/shared/api";
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
    case "nothing_chosen":
      return null;
    case "unsupported":
      return <p className="text-[13px] text-ink-2">{t.unsupported}</p>;
    case "too_many":
      return <p className="text-[13px] text-ink-2">{t.tooMany(outcome.limit)}</p>;
    case "compared":
      return <Report report={outcome.report} />;
    default: {
      const unknown: never = outcome;
      return unknown;
    }
  }
}

/**
 * Identical copies in folders the person picks in the system's dialog
 * (ADR-0057): the page passes no path, and nothing here removes anything.
 * Absent where the host cannot show the dialog (a browser).
 */
export function Duplicates() {
  const { m } = useI18n();
  const compare = useTransport().compareFolders;
  const title = useId();
  const [progress, setProgress] = useState<CompareProgressView | null>(null);
  const run = useMutation({
    mutationFn: (ask: NonNullable<typeof compare>) => ask(setProgress),
    onSettled: () => setProgress(null),
  });
  if (compare === undefined) return null;
  const t = m.overview.copies;
  const compared = run.data !== undefined && run.data.kind !== "nothing_chosen";
  return (
    <section aria-labelledby={title} aria-busy={run.isPending} className="space-y-3 pt-8">
      <div className="flex items-baseline justify-between gap-3">
        <h2 id={title} className="text-section text-ink">
          {t.title}
        </h2>
        <button
          type="button"
          disabled={run.isPending}
          onClick={() => run.mutate(compare)}
          className={buttonClass("secondary", "sm")}
        >
          {run.isPending ? t.comparing : compared ? t.chooseAgain : t.choose}
        </button>
      </div>
      {!compared && !run.isPending && !run.isError && (
        <p className="text-[13px] text-ink-2">{t.intro}</p>
      )}
      {progress !== null && (
        <p role="status" className="text-[13px] text-ink-2 tabular-nums">
          {t.stage[progress.stage]} · {t.filesRead(progress.files_read)}
        </p>
      )}
      {run.isError && <ErrorNotice message={t.failed} error={run.error} />}
      {!run.isPending && run.data !== undefined && <Outcome outcome={run.data} />}
    </section>
  );
}
