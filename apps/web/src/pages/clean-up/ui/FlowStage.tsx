import { useI18n } from "@/shared/i18n";

/** The person's steps on Explore, not the engine's stages or a percentage. */
export type Step = "scan" | "results" | "cleanUp";

const STEPS: readonly Step[] = ["scan", "results", "cleanUp"];

/**
 * Where the person is in Explore: scan (and the decisions that follow it),
 * see the results, clean up. Earlier steps are joined by a solid rule, later
 * ones by a dashed one; the current step is the one in ink.
 */
export function FlowStage({ step }: { readonly step: Step }) {
  const { m } = useI18n();
  const current = STEPS.indexOf(step);
  const labels: { readonly [S in Step]: string } = {
    scan: m.workspace.scanStage,
    results: m.workspace.resultsStage,
    cleanUp: m.workspace.cleanUpStage,
  };
  return (
    <ol aria-label={m.workspace.workflow} className="flex max-w-xl items-center gap-3 text-[12px]">
      {STEPS.map((each, index) => (
        <li
          key={each}
          aria-current={index === current ? "step" : undefined}
          className="flex min-w-0 flex-1 items-center gap-2 last:flex-none"
        >
          <span
            className={`border-b-2 pb-1 font-mono text-[12px] ${index === current ? "border-accent text-accent-ink" : index < current ? "border-transparent text-ink-2" : "border-transparent text-ink-3"}`}
          >
            {String(index + 1).padStart(2, "0")}
          </span>
          <span
            className={`whitespace-nowrap ${index === current ? "font-medium text-ink" : index < current ? "text-ink-2" : "text-ink-3"}`}
          >
            {labels[each]}
          </span>
          {index < STEPS.length - 1 && (
            <span
              aria-hidden="true"
              className={`ml-1 min-w-2 flex-1 border-t ${index < current ? "border-line-strong" : "border-dashed border-line-strong"}`}
            />
          )}
        </li>
      ))}
    </ol>
  );
}
