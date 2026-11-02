import { useQuery } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { useId } from "react";
import { useScanSession } from "@/entities/scan-session";
import { QUERY_KEYS, useTransport } from "@/shared/api";
import { useI18n } from "@/shared/i18n";
import { formatBytes, sumBytes } from "@/shared/lib";
import { Bytes, buttonClass, ErrorNotice, Notice } from "@/shared/ui";
import { assessedPermille, diskUsage, percentText } from "../model/disk";

/**
 * The disk as one band (docs/design/visual-language.md, storage topography):
 * used space filled, free space open, and a bracket under the share the last
 * assessment's items take, linking to the map of them on Explore. Shares are
 * in tenths of a percent.
 */
function DiskStrip({
  used,
  assessed,
  label,
}: {
  readonly used: number | null;
  readonly assessed: number;
  /** What the bracket says; `null` without an assessment. */
  readonly label: string | null;
}) {
  const end = used ?? 0;
  const start = end - assessed;
  // The label hangs from the bracket's far end, on whichever side has room.
  const anchor = end >= 500 ? { right: `${100 - end / 10}%` } : { left: `${start / 10}%` };
  return (
    <div>
      <div
        aria-hidden="true"
        className="h-3 overflow-hidden rounded-[2px] bg-canvas outline-1 -outline-offset-1 outline-line-strong"
      >
        {used !== null && (
          <div className="h-full bg-line-strong" style={{ width: `${used / 10}%` }} />
        )}
      </div>
      {label !== null && assessed > 0 && (
        <div className="relative mt-1 h-9">
          <span
            aria-hidden="true"
            className="absolute top-0 h-1.5 min-w-1 border-x border-b border-accent"
            style={{ left: `${start / 10}%`, width: `${assessed / 10}%` }}
          />
          <Link
            to="/clean"
            className="absolute top-3 rounded-sm text-[12px] whitespace-nowrap text-accent-ink tabular-nums hover:underline focus-visible:outline-2 focus-visible:outline-accent"
            style={anchor}
          >
            {label}
          </Link>
        </div>
      )}
    </div>
  );
}

/** Physical disk usage; kept separate from the sizes of assessed candidates. */
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
  const session = useScanSession();
  const running = session.scanning || (session.scan !== null && session.assessment === "pending");
  const assessed =
    session.candidates === null || running
      ? null
      : sumBytes(session.candidates.map((c) => c.reclaimable_bytes));
  const d = m.overview.disk;
  const none = "–";
  return (
    <section
      aria-labelledby={title}
      aria-busy={disk.isPending}
      className="space-y-5 border-b border-line pb-8"
    >
      <div className="flex items-baseline justify-between gap-3">
        <h2 id={title} className="text-section text-ink">
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
          {usage !== null && (
            <meter
              className="sr-only"
              aria-label={d.meter}
              min={0}
              max={1000}
              value={usage.usedPermille}
              high={900}
              optimum={0}
            >
              {percent === null ? m.common.loading : d.percentUsed(percent)}
            </meter>
          )}
          <div>
            <p className="text-[13px] text-ink-2">{m.workspace.available}</p>
            <p className="mt-1 text-measure text-ink tabular-nums">
              {usage === null ? none : <Bytes value={usage.available} />}
            </p>
            <p className="mt-3 flex flex-wrap gap-x-4 gap-y-1 text-[12px] text-ink-2 tabular-nums">
              <span>{d.used(usage === null ? none : formatBytes(usage.used))}</span>
              <span>{d.total(usage === null ? none : formatBytes(usage.total))}</span>
            </p>
          </div>
          <DiskStrip
            used={usage?.usedPermille ?? null}
            assessed={usage === null || assessed === null ? 0 : assessedPermille(usage, assessed)}
            label={assessed === null ? null : d.assessed(formatBytes(assessed))}
          />
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
