import { useQuery } from "@tanstack/react-query";
import { useId } from "react";
import { QUERY_KEYS, useTransport } from "@/shared/api";
import { useI18n } from "@/shared/i18n";
import { formatBytes } from "@/shared/lib";
import { buttonClass, ErrorNotice, Notice } from "@/shared/ui";
import { diskUsage, percentText } from "../model/disk";

/** A measured capacity field: every division has the same physical-space scale. */
function CapacityField({ used }: { readonly used: number | null }) {
  const clip = useId();
  const divisions = Array.from({ length: 64 }, (_, index) => index);
  const boundary = ((used ?? 0) * 768) / 1000;
  return (
    <svg
      aria-hidden="true"
      viewBox="0 0 768 144"
      className="h-36 w-full"
      preserveAspectRatio="none"
    >
      <defs>
        <clipPath id={clip}>
          <rect width={boundary} height={144} />
        </clipPath>
      </defs>
      {divisions.map((index) => (
        <rect
          key={index}
          x={index * 12}
          y={12}
          width={9}
          height={112}
          fill="var(--lm-meter-track)"
        />
      ))}
      {used !== null && (
        <>
          <g clipPath={`url(#${clip})`}>
            {divisions.map((index) => (
              <rect
                key={index}
                x={index * 12}
                y={12}
                width={9}
                height={112}
                fill="var(--lm-ink)"
                opacity={0.64}
              />
            ))}
          </g>
          <path d={`M${boundary} 4V132`} stroke="var(--lm-accent)" strokeWidth={2} />
          <path
            d={`m${boundary - 4} 0 4 4 4-4`}
            fill="none"
            stroke="var(--lm-accent)"
            strokeWidth={2}
          />
        </>
      )}
    </svg>
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
  const d = m.overview.disk;
  const none = "–";
  return (
    <section
      aria-labelledby={title}
      aria-busy={disk.isPending}
      className="space-y-5 border-b border-line pb-8"
    >
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
          <div className="grid items-center gap-6 min-[800px]:grid-cols-[0.8fr_1.2fr] min-[800px]:gap-16">
            <div>
              <p className="text-[13px] text-ink-2">{m.workspace.available}</p>
              <p className="mt-1 text-[clamp(2.5rem,5vw,3.5rem)] leading-tight font-medium tracking-[-0.06em] text-ink tabular-nums">
                {usage === null ? none : formatBytes(usage.available)}
              </p>
              <p className="mt-3 flex flex-wrap gap-x-4 gap-y-1 text-[12px] text-ink-2 tabular-nums">
                <span>{d.used(usage === null ? none : formatBytes(usage.used))}</span>
                <span>{d.total(usage === null ? none : formatBytes(usage.total))}</span>
              </p>
            </div>
            <CapacityField used={usage?.usedPermille ?? null} />
          </div>
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
