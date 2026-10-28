import { useI18n } from "@/shared/i18n";

type Phase = "ready" | "scan" | "evidence" | "decision" | "done";

const POSITION: Record<Phase, number> = {
  ready: -1,
  scan: 0,
  evidence: 1,
  decision: 2,
  done: 3,
};

/** The real workflow, not a percentage or a confidence score. */
export function FlowStage({ phase }: { readonly phase: Phase }) {
  const { m } = useI18n();
  const current = POSITION[phase];
  return (
    <ol aria-label={m.workspace.workflow} className="flex max-w-xl items-center gap-3 text-[12px]">
      {[m.workspace.scanStage, m.workspace.evidenceStage, m.workspace.decisionStage].map(
        (label, index) => (
          <li
            key={label}
            aria-current={index === current ? "step" : undefined}
            className="flex min-w-0 flex-1 items-center gap-2 last:flex-none"
          >
            <span
              className={`border-b-2 pb-1 font-mono text-[11px] ${index === current ? "border-accent text-accent-ink" : "border-transparent text-ink-3"}`}
            >
              {String(index + 1).padStart(2, "0")}
            </span>
            <span
              className={`whitespace-nowrap ${index === current ? "font-medium text-ink" : "text-ink-3"}`}
            >
              {label}
            </span>
            {index < 2 && (
              <span
                aria-hidden="true"
                className={`ml-1 min-w-2 flex-1 border-t ${index < current ? "border-line-strong" : "border-dashed border-line-strong"}`}
              />
            )}
          </li>
        ),
      )}
    </ol>
  );
}
