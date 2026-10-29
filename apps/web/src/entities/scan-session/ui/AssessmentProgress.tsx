import type { AssessProgressView } from "@lumen/api-client";
import { countFormat, useI18n } from "@/shared/i18n";
import { stagePercent } from "../model/progress";

/** Stage-specific work, retained by the session when its page closes. */
export function AssessmentProgress({ progress }: { readonly progress: AssessProgressView | null }) {
  const { locale, m } = useI18n();
  const count = countFormat(locale);
  const percent = stagePercent(progress);
  const label = progress ? m.scanning.stages[progress.stage] : m.scanning.deciding;
  const units =
    progress?.total === null || !progress
      ? null
      : m.scanning.processed(count(progress.completed), count(progress.total));
  const percentage = percent === null ? null : m.scanning.stagePercent(percent);
  return (
    <div className="space-y-3">
      <div className="flex flex-wrap items-baseline justify-between gap-x-5 gap-y-1 text-[13px]">
        <p className="text-ink-2">{label}</p>
        {percentage !== null && <p className="font-medium tabular-nums text-ink">{percentage}</p>}
      </div>
      {percent === null ? (
        <div role="progressbar" aria-label={label} className="h-0.5 overflow-hidden bg-accent-soft">
          <div className="h-full w-2/5 animate-[indeterminate_1.4s_ease-in-out_infinite] bg-accent motion-reduce:w-full motion-reduce:animate-none motion-reduce:opacity-50" />
        </div>
      ) : (
        <progress
          className="assessment-progress"
          max={100}
          value={percent}
          aria-label={label}
          aria-valuetext={`${percentage} · ${units}`}
        />
      )}
      {units !== null && <p className="text-[12px] tabular-nums text-ink-3">{units}</p>}
    </div>
  );
}
