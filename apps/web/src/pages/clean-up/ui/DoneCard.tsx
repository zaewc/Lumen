import type { QuarantineReportView } from "@lumen/api-client";
import { Link } from "@tanstack/react-router";
import { useI18n } from "@/shared/i18n";
import { baseName, formatBytes } from "@/shared/lib";
import { buttonClass, Card, CheckIcon, Disclosure, Notice } from "@/shared/ui";
import { outcomeText } from "../lib/outcome";
import { reportSummary } from "../model/report";
import { StepHeading } from "./StepHeading";

/** What the clean-up did, where the items are now, and what comes next. */
export function DoneCard({
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
    <Card className="space-y-6 sm:p-8">
      <div className="flex gap-4">
        <span className="flex size-10 shrink-0 items-center justify-center rounded-full bg-emerald-100 text-emerald-700 dark:bg-emerald-950 dark:text-emerald-300">
          <CheckIcon />
        </span>
        <div className="min-w-0">
          <StepHeading focus={focus} className="text-xl font-semibold tracking-tight">
            {moved > 0 ? m.done.title(moved, formatBytes(report.moved_bytes)) : m.done.noneMoved}
          </StepHeading>
          {moved > 0 && (
            <p className="mt-1 text-neutral-600 dark:text-neutral-400">{m.done.freedNote}</p>
          )}
        </div>
      </div>
      {report.warnings.map((warning) => (
        <Notice key={warning} tone="warning">
          <p>{warning}</p>
        </Notice>
      ))}
      <div className="flex flex-col gap-2 sm:flex-row">
        <Link to="/quarantine" className={buttonClass("primary")}>
          {m.done.openQuarantine}
        </Link>
        <button type="button" onClick={onStartOver} className={buttonClass("secondary")}>
          {m.common.scanAgain}
        </button>
      </div>
      {notMoved.length > 0 && (
        <Disclosure
          className="border-t border-neutral-100 pt-4 dark:border-neutral-800"
          summary={
            <span className="text-sm font-medium text-neutral-700 dark:text-neutral-300">
              {m.done.notMoved(notMoved.length)}
            </span>
          }
        >
          <ul className="divide-y divide-neutral-100 dark:divide-neutral-800">
            {notMoved.map((item) => (
              <li key={item.path} className="py-3 text-sm">
                <p className="font-semibold break-words text-neutral-900 dark:text-neutral-100">
                  {baseName(item.path)}
                </p>
                <p className="mt-0.5 text-neutral-700 dark:text-neutral-300">
                  {outcomeText(m, item.result)}
                </p>
                <p className="mt-1 font-mono text-xs break-all text-neutral-500 dark:text-neutral-400">
                  {item.path}
                </p>
              </li>
            ))}
          </ul>
        </Disclosure>
      )}
    </Card>
  );
}
