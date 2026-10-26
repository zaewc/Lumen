import { Link } from "@tanstack/react-router";
import { useId } from "react";
import { heldTotal, useHeld } from "@/entities/held";
import { summarize, useScanSession } from "@/entities/scan-session";
import { countFormat, useI18n } from "@/shared/i18n";
import { formatBytes } from "@/shared/lib";
import { ChevronIcon, Notice } from "@/shared/ui";

/** A row of the summary: the whole row is a link to the details. */
const ROW =
  "-mx-2 grid grid-cols-[minmax(0,1fr)_auto_5.5rem_auto] items-center gap-x-4 rounded-md px-2 py-2 hover:bg-fill focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-accent";

/** What a summary row shows: what it is, how many, how large. */
function RowContent({
  label,
  count,
  size,
  attention = false,
}: {
  readonly label: string;
  readonly count: string;
  readonly size: string;
  /** The count needs the user's attention. */
  readonly attention?: boolean;
}) {
  return (
    <>
      <span className="truncate text-ink">{label}</span>
      <span
        className={`tabular-nums ${attention ? "text-amber-700 dark:text-amber-400" : "text-ink-2"}`}
      >
        {count}
      </span>
      <span className="text-right tabular-nums text-ink">{size}</span>
      <ChevronIcon className="text-ink-3" />
    </>
  );
}

/** Quarantine's row: how many items it holds and what removing them would free. */
function QuarantineRow() {
  const { m } = useI18n();
  const held = useHeld();
  const items = held.data ?? [];
  const total = heldTotal(items);
  const size = formatBytes(total.bytes);
  return (
    <li>
      <Link to="/quarantine" className={ROW}>
        <RowContent
          label={m.overview.session.quarantine}
          count={m.common.items(items.length)}
          size={total.exact ? size : m.overview.session.atLeast(size)}
        />
      </Link>
    </li>
  );
}

/** A scan running in this session, with a way to its live progress. */
function Running() {
  const { locale, m } = useI18n();
  const session = useScanSession();
  const count = countFormat(locale);
  return (
    <div role="status" className="space-y-2">
      <p className="flex flex-wrap items-baseline gap-x-3">
        <span className="text-ink">
          {session.scanning ? m.common.scanning : m.scanning.deciding}
        </span>
        {session.progress !== null && (
          <span className="tabular-nums text-ink-2">
            {m.overview.session.found(count(session.progress.entries))}
          </span>
        )}
        <Link to="/clean" className="ml-auto text-accent-ink hover:underline">
          {m.overview.session.showProgress}
        </Link>
      </p>
      <div className="h-1 overflow-hidden rounded-full bg-accent-soft">
        <div className="h-full w-2/5 animate-[indeterminate_1.6s_ease-in-out_infinite] rounded-full bg-accent motion-reduce:w-full motion-reduce:animate-none motion-reduce:opacity-50" />
      </div>
    </div>
  );
}

/**
 * This session's last scan as a short list (what can be cleaned up, what to
 * clean up with a tool's own command when there is any, what needs review,
 * what Quarantine holds), each row a link to its details; or what to do when
 * there is none.
 */
export function LastScan() {
  const { m } = useI18n();
  const session = useScanSession();
  const held = useHeld();
  const title = useId();
  const running = session.scanning || (session.scan !== null && session.assessment === "pending");
  const summary = session.candidates === null ? null : summarize(session.candidates);
  const report = session.report;
  const anyHeld = (held.data?.length ?? 0) > 0;
  const s = m.overview.session;
  return (
    <section aria-labelledby={title} className="space-y-2">
      <h2 id={title} className="text-[13px] font-semibold text-ink">
        {s.title}
      </h2>
      {running && <Running />}
      {!running && session.scanError !== null && (
        <Notice tone="warning">
          <p>{m.start.failed}</p>
        </Notice>
      )}
      {!running && report !== null && (
        <Notice tone="success">
          <p>
            {s.cleaned(
              report.items.filter((item) => item.result.kind === "quarantined").length,
              formatBytes(report.moved_bytes),
            )}
          </p>
        </Notice>
      )}
      {!running && summary === null && report === null && session.scanError === null && (
        <p className="text-ink-2">{s.none}</p>
      )}
      {(summary !== null || anyHeld) && (
        <ul className="divide-y divide-line border-y border-line">
          {summary !== null && (
            <>
              <li>
                <Link to="/clean" search={{ show: "quarantine" }} className={ROW}>
                  <RowContent
                    label={s.canFree}
                    count={m.common.items(summary.quarantine.items.length)}
                    size={formatBytes(summary.quarantine.bytes)}
                  />
                </Link>
              </li>
              {summary.command.items.length > 0 && (
                <li>
                  <Link to="/clean" search={{ show: "command" }} className={ROW}>
                    <RowContent
                      label={s.command}
                      count={m.common.items(summary.command.items.length)}
                      size={formatBytes(summary.command.bytes)}
                    />
                  </Link>
                </li>
              )}
              <li>
                <Link to="/clean" search={{ show: "review" }} className={ROW}>
                  <RowContent
                    label={s.review}
                    count={m.common.items(summary.review.items.length)}
                    size={formatBytes(summary.review.bytes)}
                    attention={summary.review.items.length > 0}
                  />
                </Link>
              </li>
            </>
          )}
          <QuarantineRow />
        </ul>
      )}
    </section>
  );
}
