import type { QuarantineReportView } from "@lumen/api-client";
import { Link } from "@tanstack/react-router";
import { useI18n } from "@/shared/i18n";
import { baseName, formatBytes } from "@/shared/lib";
import { buttonClass, CheckIcon, Notice } from "@/shared/ui";
import { outcomeText } from "../lib/outcome";
import { reportSummary } from "../model/report";
import { StepHeading } from "./StepHeading";

/** What the clean-up did, where the items are now, and what was not moved and why. */
export function DonePanel({
  report,
  focus,
}: {
  readonly report: QuarantineReportView;
  readonly focus: boolean;
}) {
  const { m } = useI18n();
  const { moved, notMoved } = reportSummary(report);
  return (
    <>
      <section className="space-y-3">
        <div className="space-y-1">
          <div className="flex items-center gap-2">
            {moved > 0 && <CheckIcon className="size-4 text-emerald-600 dark:text-emerald-400" />}
            <StepHeading focus={focus} className="text-[15px] font-semibold text-ink">
              {moved > 0 ? m.done.title(moved, formatBytes(report.moved_bytes)) : m.done.noneMoved}
            </StepHeading>
          </div>
          {moved > 0 && <p className="text-ink-2">{m.done.freedNote}</p>}
        </div>
        <Link to="/quarantine" className={buttonClass("primary")}>
          {m.done.openQuarantine}
        </Link>
      </section>
      {report.warnings.map((warning) => (
        <Notice key={warning} tone="warning">
          <p>{warning}</p>
        </Notice>
      ))}
      {notMoved.length > 0 && (
        <section>
          <h3 className="pb-1 font-semibold text-ink">{m.done.notMoved(notMoved.length)}</h3>
          <ul className="divide-y divide-line border-y border-line">
            {notMoved.map((item) => (
              <li key={item.path} className="py-2">
                <p className="font-medium break-words text-ink">{baseName(item.path)}</p>
                <p className="text-ink-2">{outcomeText(m, item.result)}</p>
                <p className="font-mono text-[11px] break-all text-ink-3 select-text">
                  {item.path}
                </p>
              </li>
            ))}
          </ul>
        </section>
      )}
    </>
  );
}
