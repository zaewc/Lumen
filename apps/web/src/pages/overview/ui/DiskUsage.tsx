import { useQuery } from "@tanstack/react-query";
import { useId } from "react";
import { QUERY_KEYS, useTransport } from "@/shared/api";
import { useI18n } from "@/shared/i18n";
import { formatBytes } from "@/shared/lib";
import { buttonClass, ErrorNotice, Notice } from "@/shared/ui";
import { diskUsage, percentText } from "../model/disk";

/** How full the startup disk is: one plain bar and its figures. */
export function DiskUsage() {
  const { m } = useI18n();
  const transport = useTransport();
  const title = useId();
  const disk = useQuery({
    queryKey: QUERY_KEYS.disk,
    queryFn: () => transport.disk(),
    retry: false,
  });
  const usage = disk.isSuccess ? diskUsage(disk.data) : null;
  const percent = usage === null ? null : percentText(usage.usedPermille);
  const low = usage?.low ?? false;
  const d = m.overview.disk;
  const none = "–";
  return (
    <section aria-labelledby={title} className="space-y-2">
      <div className="flex items-baseline justify-between gap-3">
        <h2 id={title} className="text-[13px] font-semibold text-ink">
          {d.title}
        </h2>
        {percent !== null && (
          <span className="text-[13px] tabular-nums text-ink-2">{d.percentUsed(percent)}</span>
        )}
      </div>
      {disk.isError ? (
        <ErrorNotice message={d.loadFailed} error={disk.error}>
          <button
            type="button"
            onClick={() => void disk.refetch()}
            className={buttonClass("secondary", "sm")}
          >
            {m.common.tryAgain}
          </button>
        </ErrorNotice>
      ) : (
        <>
          {/* The native meter carries the value for assistive technology; the bar draws it. */}
          <meter
            className="sr-only"
            aria-label={d.meter}
            min={0}
            max={1000}
            value={usage?.usedPermille ?? 0}
            high={900}
            optimum={0}
          >
            {percent === null ? m.common.loading : d.percentUsed(percent)}
          </meter>
          <div aria-hidden="true" className="h-2 overflow-hidden rounded-full bg-meter-track">
            <div
              className={`h-full rounded-full ${low ? "bg-amber-500" : "bg-accent"}`}
              style={{ width: `${usage === null ? 0 : usage.usedPermille / 10}%` }}
            />
          </div>
          <p className="flex flex-wrap gap-x-4 gap-y-0.5 text-[13px] tabular-nums text-ink-2">
            <span>{d.used(usage === null ? none : formatBytes(usage.used))}</span>
            <span>{d.available(usage === null ? none : formatBytes(usage.available))}</span>
            <span>{d.total(usage === null ? none : formatBytes(usage.total))}</span>
          </p>
          {low && (
            <Notice tone="warning">
              <p>{d.low}</p>
            </Notice>
          )}
        </>
      )}
    </section>
  );
}
