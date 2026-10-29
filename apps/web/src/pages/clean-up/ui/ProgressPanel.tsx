import type { AssessProgressView, ScanProgressView } from "@lumen/api-client";
import { AssessmentProgress } from "@/entities/scan-session";
import { countFormat, useI18n } from "@/shared/i18n";
import { formatBytes } from "@/shared/lib";
import { FlowStage } from "./FlowStage";
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
        className={`mt-1 text-[28px] tracking-[-0.04em] tabular-nums ${warn ? "text-amber-700 dark:text-amber-400" : "text-ink"} font-medium`}
      >
        {value}
      </dd>
    </div>
  );
}

/** A scan in progress (live totals), then the assessment that follows it. */
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
    <section className="max-w-4xl space-y-7">
      <FlowStage
        phase={
          !deciding
            ? "scan"
            : assessmentProgress?.stage === "deciding" || assessmentProgress?.stage === "describing"
              ? "decision"
              : "evidence"
        }
      />
      <div role="status" className="space-y-1">
        <StepHeading focus={focus} className="text-[24px] font-medium tracking-[-0.035em] text-ink">
          {deciding ? m.scanning.deciding : m.scanning.title}
        </StepHeading>
        <p className="text-ink-2">{deciding ? m.scanning.decidingHint : m.scanning.readOnly}</p>
      </div>
      {deciding ? (
        <AssessmentProgress progress={assessmentProgress} />
      ) : (
        <div
          role="progressbar"
          aria-label={m.scanning.progress}
          className="h-0.5 overflow-hidden bg-accent-soft"
        >
          <div className="h-full w-2/5 animate-[indeterminate_1.4s_ease-in-out_infinite] bg-accent motion-reduce:w-full motion-reduce:animate-none motion-reduce:opacity-50" />
        </div>
      )}
      <dl className="flex flex-wrap gap-x-12 gap-y-4">
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
        <p className="flex min-w-0 items-baseline gap-2 text-[12px]">
          <span className="shrink-0 text-ink-3">{m.scanning.reading}</span>
          <span
            className="min-w-0 truncate font-mono text-[11px] text-ink-2"
            title={progress.current}
          >
            {progress.current}
          </span>
        </p>
      )}
    </section>
  );
}
