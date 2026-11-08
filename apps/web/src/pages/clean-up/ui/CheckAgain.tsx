import type { CandidateView, ReassessGapView, ReassessView } from "@lumen/api-client";
import { useMutation } from "@tanstack/react-query";
import { type Status, statusOf, useScanSession } from "@/entities/scan-session";
import { useTransport } from "@/shared/api";
import { useI18n } from "@/shared/i18n";
import { buttonClass, ErrorNotice } from "@/shared/ui";
import { recheckedOf } from "../model/recheck";

/** The sentence for how checking `id` again ended, from `before`. */
function useOutcomeText(): (view: ReassessView, id: string, before: Status) => string {
  const { m } = useI18n();
  const t = m.detail.recheck;
  return (view, id, before) => {
    const rechecked = recheckedOf(view, id, before);
    switch (rechecked.kind) {
      case "settled":
        return t.settled;
      case "changed":
        return t.changed;
      case "still":
        return t.still(m.results.status[rechecked.status]);
      case "now":
        return t.now(m.results.status[rechecked.before], m.results.status[rechecked.after]);
      case "missing":
        return t.failed;
      default: {
        const unknown: never = rechecked;
        return unknown;
      }
    }
  };
}

/**
 * What Lumen did not know when it decided about `candidate`, and a way to
 * check it again (ADR-0058): only reads; an item decided again replaces the
 * earlier one in the results. Absent where the host cannot check (a browser),
 * and for decisions that rest on nothing unknown.
 */
export function CheckAgain({ candidate }: { readonly candidate: CandidateView }) {
  const { m } = useI18n();
  const t = m.detail.recheck;
  const reassess = useTransport().reassess;
  const session = useScanSession();
  const outcome = useOutcomeText();
  const run = useMutation({
    mutationFn: ({ ask }: { ask: NonNullable<typeof reassess>; before: Status }) =>
      ask({ ids: [candidate.id] }),
    onSuccess: (view) => {
      session.replace(
        view.items.flatMap((item) =>
          item.outcome.kind === "decided" ? [item.outcome.candidate] : [],
        ),
      );
    },
  });
  const gaps: readonly ReassessGapView[] = candidate.rechecks ?? [];
  if (reassess === undefined || (gaps.length === 0 && run.data === undefined)) return null;
  return (
    <section aria-busy={run.isPending}>
      <h3 className="text-section text-ink">{t.action}</h3>
      {gaps.length > 0 && (
        <>
          <p className="mt-2 text-[14px] text-ink-2">{t.unknown}</p>
          <ul className="mt-1 list-disc pl-5 text-[14px] text-ink">
            {gaps.map((gap) => (
              <li key={gap}>{t.gaps[gap]}</li>
            ))}
          </ul>
          <button
            type="button"
            disabled={run.isPending}
            onClick={() => run.mutate({ ask: reassess, before: statusOf(candidate) })}
            className={`mt-3 ${buttonClass("secondary", "sm")}`}
          >
            {run.isPending ? t.checking : t.action}
          </button>
          <p className="mt-2 text-[12px] text-ink-3">{t.note}</p>
        </>
      )}
      {run.data !== undefined && (
        <p role="status" className="mt-3 text-[14px] text-ink">
          {outcome(run.data, candidate.id, run.variables.before)}
        </p>
      )}
      {run.isError && <ErrorNotice message={t.failed} error={run.error} />}
    </section>
  );
}
