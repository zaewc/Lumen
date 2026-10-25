import { useQuery } from "@tanstack/react-query";
import { useId } from "react";
import { QUERY_KEYS, useTransport } from "@/shared/api";
import { useI18n } from "@/shared/i18n";
import { formatBytes } from "@/shared/lib";
import { ErrorNotice, WarningIcon } from "@/shared/ui";
import { diskUsage, percentText } from "../model/disk";

/** One figure under the bar: a swatch, what it is, and how much. */
function Figure({
  swatch,
  label,
  value,
}: {
  readonly swatch: string | null;
  readonly label: string;
  readonly value: string;
}) {
  return (
    <div className="min-w-0">
      <dt className="flex items-center gap-1.5 text-[12px] text-ink-2">
        {swatch !== null && (
          <span aria-hidden="true" className={`size-2.5 shrink-0 rounded-[3px] ${swatch}`} />
        )}
        {label}
      </dt>
      <dd className="mt-0.5 text-[15px] font-semibold tabular-nums text-ink">{value}</dd>
    </div>
  );
}

/** How full the startup disk is, as Disk Utility shows it: one bar and three figures. */
export function DiskCard() {
  const { m } = useI18n();
  const transport = useTransport();
  const title = useId();
  const disk = useQuery({
    queryKey: QUERY_KEYS.disk,
    queryFn: () => transport.disk(),
    retry: false,
  });
  if (disk.isError) {
    return <ErrorNotice message={m.overview.disk.loadFailed} error={disk.error} />;
  }
  const usage = disk.isSuccess ? diskUsage(disk.data) : null;
  const percent = usage === null ? null : percentText(usage.usedPermille);
  const low = usage?.low ?? false;
  return (
    <section
      aria-labelledby={title}
      className="rounded-xl border border-line bg-surface p-5 min-[720px]:p-6"
    >
      <div className="flex items-center gap-3">
        <h2 id={title} className="min-w-0 flex-1 text-[15px] font-semibold text-ink">
          {m.overview.disk.title}
        </h2>
        {percent !== null && (
          <span className="text-[13px] tabular-nums text-ink-2">
            {m.overview.disk.percentUsed(percent)}
          </span>
        )}
      </div>
      {/* The native meter carries the value for assistive technology; the bar draws it. */}
      <meter
        className="sr-only"
        aria-label={m.overview.disk.meter}
        min={0}
        max={1000}
        value={usage?.usedPermille ?? 0}
        high={900}
        optimum={0}
      >
        {percent === null ? m.common.loading : m.overview.disk.percentUsed(percent)}
      </meter>
      <div aria-hidden="true" className="mt-4 h-6 overflow-hidden rounded-lg bg-meter-track">
        <div
          className={`h-full rounded-lg transition-[width] duration-700 ease-out motion-reduce:transition-none ${low ? "bg-linear-to-r from-amber-500 to-orange-500" : "bg-linear-to-r from-accent to-[#8b8bff]"}`}
          style={{ width: `${usage === null ? 0 : usage.usedPermille / 10}%` }}
        />
      </div>
      <dl className="mt-4 grid grid-cols-3 gap-3">
        <Figure
          swatch={low ? "bg-amber-500" : "bg-accent"}
          label={m.overview.disk.used}
          value={usage === null ? "–" : formatBytes(usage.used)}
        />
        <Figure
          swatch="bg-meter-track ring-1 ring-inset ring-line-strong"
          label={m.overview.disk.available}
          value={usage === null ? "–" : formatBytes(usage.available)}
        />
        <Figure
          swatch={null}
          label={m.overview.disk.total}
          value={usage === null ? "–" : formatBytes(usage.total)}
        />
      </dl>
      {low && (
        <div className="mt-4 flex gap-2.5 rounded-lg bg-amber-500/10 px-3.5 py-3 text-[13px] text-amber-950 dark:bg-amber-400/10 dark:text-amber-100">
          <WarningIcon className="mt-px size-[18px] text-amber-600 dark:text-amber-400" />
          <div>
            <p className="font-semibold">{m.overview.disk.low}</p>
            <p className="mt-0.5 text-amber-900/80 dark:text-amber-100/80">
              {m.overview.disk.lowHint}
            </p>
          </div>
        </div>
      )}
    </section>
  );
}
