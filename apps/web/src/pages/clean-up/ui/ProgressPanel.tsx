import type { AssessProgressView, ScanProgressView } from "@lumen/api-client";
import { AssessmentProgress } from "@/entities/scan-session";
import { countFormat, useI18n } from "@/shared/i18n";
import { FlowStage } from "./FlowStage";
import { MapFrame } from "./MapFrame";
import { StepHeading } from "./StepHeading";

/** One live figure of the scan. */
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
    <div className="min-w-0">
      <dt className="text-[12px] text-ink-2">{label}</dt>
      <dd
        className={`mt-1 text-figure tabular-nums ${warn ? "text-amber-700 dark:text-amber-400" : "text-ink"} font-medium`}
      >
        {value}
      </dd>
    </div>
  );
}

/**
 * A scan in progress (live totals, and the map's frame filling as it
 * reads, which carries its size on disk), then the assessment that follows.
 */
export function ProgressPanel({
  progress,
  assessmentProgress,
  deciding,
  focus,
}: {
  readonly progress: ScanProgressView | null;
  readonly assessmentProgress: AssessProgressView | null;
  readonly deciding: boolean;
  readonly focus: boolean;
}) {
  const { locale, m } = useI18n();
  const count = countFormat(locale);
  const none = "–";
  return (
    <section className="space-y-7">
      <FlowStage step="scan" />
      <div role="status" className="space-y-1">
        <StepHeading focus={focus} className="text-section text-ink">
          {deciding ? m.scanning.deciding : m.scanning.title}
        </StepHeading>
        <p className="text-ink-2">{deciding ? m.scanning.decidingHint : m.scanning.readOnly}</p>
      </div>
      {deciding && (
        <div className="max-w-4xl">
          <AssessmentProgress progress={assessmentProgress} />
        </div>
      )}
      <dl className="flex flex-wrap gap-x-12 gap-y-4">
        <Stat label={m.scanning.items} value={progress ? count(progress.entries) : none} />
        <Stat
          label={m.scanning.unreadable}
          value={progress ? count(progress.unreadable) : none}
          warn={progress !== null && progress.unreadable !== "0"}
        />
      </dl>
      {!deciding && progress?.current && (
        <p className="flex min-w-0 items-baseline gap-2 text-[12px]">
          <span className="shrink-0 text-ink-3">{m.scanning.reading}</span>
          <span
            className="min-w-0 truncate font-mono text-[12px] text-ink-2"
            title={progress.current}
          >
            {progress.current}
          </span>
        </p>
      )}
      <MapFrame read={progress?.allocated_bytes ?? "0"} />
    </section>
  );
}
