import type { ScanProgressView } from "@lumen/api-client";
import { countFormat, useI18n } from "@/shared/i18n";
import { formatBytes } from "@/shared/lib";
import { Card } from "@/shared/ui";
import { StepHeading } from "./StepHeading";

function Stat({
  label,
  value,
  warn = false,
}: {
  readonly label: string;
  readonly value: string;
  readonly warn?: boolean;
}) {
  return (
    <div className="min-w-0 rounded-xl bg-neutral-50 p-3">
      <dt className="text-xs text-neutral-500">{label}</dt>
      <dd
        className={`mt-1 truncate text-base font-semibold tabular-nums sm:text-lg ${warn ? "text-amber-700" : "text-neutral-900"}`}
      >
        {value}
      </dd>
    </div>
  );
}

/** A scan in progress (live totals), then the assessment that follows it. */
export function ScanningCard({
  progress,
  deciding,
  focus,
}: {
  readonly progress: ScanProgressView | null;
  readonly deciding: boolean;
  readonly focus: boolean;
}) {
  const { locale, m } = useI18n();
  const count = countFormat(locale);
  const none = "–";
  return (
    <Card>
      <div role="status">
        <StepHeading focus={focus} className="text-xl font-semibold tracking-tight">
          {deciding ? m.scanning.deciding : m.scanning.title}
        </StepHeading>
      </div>
      <p className="mt-1 text-sm text-neutral-600">{m.scanning.readOnly}</p>
      <div
        role="progressbar"
        aria-label={m.scanning.progress}
        className="mt-6 h-2 overflow-hidden rounded-full bg-indigo-100"
      >
        <div className="h-full w-2/5 animate-[indeterminate_1.6s_ease-in-out_infinite] rounded-full bg-indigo-600 motion-reduce:w-full motion-reduce:animate-none motion-reduce:opacity-50" />
      </div>
      <dl className="mt-6 grid grid-cols-3 gap-2 sm:gap-3">
        <Stat label={m.scanning.items} value={progress ? count(progress.entries) : none} />
        <Stat
          label={m.scanning.size}
          value={progress ? formatBytes(progress.allocated_bytes) : none}
        />
        <Stat
          label={m.scanning.unreadable}
          value={progress ? count(progress.unreadable) : none}
          warn={progress !== null && progress.unreadable !== "0"}
        />
      </dl>
      {!deciding && progress?.current && (
        <p className="mt-4 truncate font-mono text-xs text-neutral-500" title={progress.current}>
          {m.scanning.reading(progress.current)}
        </p>
      )}
    </Card>
  );
}
