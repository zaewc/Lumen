import type { ScanProgressView } from "@lumen/api-client";
import { countFormat, useI18n } from "@/shared/i18n";
import { formatBytes } from "@/shared/lib";
import { CheckIcon } from "@/shared/ui";
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
    <div className="min-w-0 px-3 py-2.5 first:pl-0 last:pr-0">
      <dt className="truncate text-[12px] text-ink-2">{label}</dt>
      <dd
        className={`mt-0.5 truncate text-[17px] font-semibold tabular-nums ${warn ? "text-amber-700 dark:text-amber-400" : "text-ink"}`}
      >
        {value}
      </dd>
    </div>
  );
}

/** One step of the two (scan, then decide): done, current, or still to come. */
function StepMark({
  number,
  label,
  state,
}: {
  readonly number: number;
  readonly label: string;
  readonly state: "done" | "current" | "next";
}) {
  const mark = {
    done: "bg-accent text-white",
    current: "bg-accent text-white ring-4 ring-accent-soft",
    next: "bg-fill-strong text-ink-3",
  }[state];
  return (
    <li
      aria-current={state === "current" ? "step" : undefined}
      className={`flex items-center gap-2 text-[13px] font-medium ${state === "next" ? "text-ink-3" : "text-ink"}`}
    >
      <span
        className={`flex size-6 items-center justify-center rounded-full text-[12px] font-semibold ${mark}`}
      >
        {state === "done" ? <CheckIcon className="size-4" /> : number}
      </span>
      {label}
    </li>
  );
}

/** A scan in progress (live totals), then the assessment that follows it. */
export function ProgressPanel({
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
    <section className="rounded-xl border border-line bg-surface p-5 min-[720px]:p-7">
      <ol className="flex items-center gap-3">
        <StepMark number={1} label={m.scanning.stepScan} state={deciding ? "done" : "current"} />
        <li aria-hidden="true" className="h-px w-8 bg-line-strong" />
        <StepMark number={2} label={m.scanning.stepDecide} state={deciding ? "current" : "next"} />
      </ol>
      <div role="status" className="mt-6">
        <StepHeading focus={focus} className="text-[20px] font-bold tracking-tight text-ink">
          {deciding ? m.scanning.deciding : m.scanning.title}
        </StepHeading>
      </div>
      <p className="mt-1 text-[13px] text-ink-2">
        {deciding ? m.scanning.decidingHint : m.scanning.readOnly}
      </p>
      <div
        role="progressbar"
        aria-label={m.scanning.progress}
        className="mt-5 h-1.5 overflow-hidden rounded-full bg-accent-soft"
      >
        <div className="h-full w-2/5 animate-[indeterminate_1.4s_ease-in-out_infinite] rounded-full bg-accent motion-reduce:w-full motion-reduce:animate-none motion-reduce:opacity-50" />
      </div>
      <dl className="mt-5 grid grid-cols-3 divide-x divide-line">
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
        <p className="mt-4 flex min-w-0 items-baseline gap-2 border-t border-line pt-3 text-[12px]">
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
