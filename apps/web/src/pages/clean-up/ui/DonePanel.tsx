import type { QuarantineReportView } from "@lumen/api-client";
import { Link } from "@tanstack/react-router";
import { useI18n } from "@/shared/i18n";
import { baseName, formatBytes } from "@/shared/lib";
import { buttonClass, CheckIcon, Notice, WarningIcon } from "@/shared/ui";
import { outcomeText } from "../lib/outcome";
import { reportSummary } from "../model/report";
import { StepHeading } from "./StepHeading";

/** What the clean-up did, where the items are now, and what comes next. */
export function DonePanel({
  report,
  onStartOver,
  focus,
}: {
  readonly report: QuarantineReportView;
  readonly onStartOver: () => void;
  readonly focus: boolean;
}) {
  const { m } = useI18n();
  const { moved, notMoved } = reportSummary(report);
  return (
    <>
      <section className="flex flex-col items-center rounded-xl border border-line bg-surface px-6 py-10 text-center">
        <span
          className={`flex size-14 items-center justify-center rounded-full ${moved > 0 ? "bg-emerald-500/15 text-emerald-600 dark:text-emerald-400" : "bg-fill-strong text-ink-2"}`}
        >
          <CheckIcon className="size-7" />
        </span>
        <StepHeading
          focus={focus}
          className="mt-5 max-w-lg text-[20px] leading-snug font-bold tracking-tight text-ink"
        >
          {moved > 0 ? m.done.title(moved, formatBytes(report.moved_bytes)) : m.done.noneMoved}
        </StepHeading>
        {moved > 0 && <p className="mt-2 text-[13px] text-ink-2">{m.done.freedNote}</p>}
        <div className="mt-7 flex flex-wrap justify-center gap-2">
          <Link to="/quarantine" className={buttonClass("primary", "lg")}>
            {m.done.openQuarantine}
          </Link>
          <button type="button" onClick={onStartOver} className={buttonClass("secondary", "lg")}>
            {m.common.scanAgain}
          </button>
        </div>
      </section>
      {report.warnings.map((warning) => (
        <Notice key={warning} tone="warning">
          <p>{warning}</p>
        </Notice>
      ))}
      {notMoved.length > 0 && (
        <section>
          <h3 className="px-1 pb-2 text-[13px] font-semibold text-ink-2">
            {m.done.notMoved(notMoved.length)}
          </h3>
          <ul className="divide-y divide-line overflow-hidden rounded-xl border border-line bg-surface">
            {notMoved.map((item) => (
              <li key={item.path} className="flex gap-3 px-4 py-3">
                <WarningIcon className="mt-0.5 size-[18px] text-amber-600 dark:text-amber-400" />
                <div className="min-w-0 flex-1 text-[13px]">
                  <p className="font-medium break-words text-ink">{baseName(item.path)}</p>
                  <p className="mt-0.5 text-ink-2">{outcomeText(m, item.result)}</p>
                  <p className="mt-1 font-mono text-[11px] break-all text-ink-3 select-text">
                    {item.path}
                  </p>
                </div>
              </li>
            ))}
          </ul>
        </section>
      )}
    </>
  );
}
