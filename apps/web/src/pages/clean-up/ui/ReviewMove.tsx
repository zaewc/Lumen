import type { CandidateView } from "@lumen/api-client";
import { useIsMutating, useMutation, useMutationState } from "@tanstack/react-query";
import { type ReactNode, useRef, useState } from "react";
import { useScanSession } from "@/entities/scan-session";
import { declined, useTransport } from "@/shared/api";
import { useI18n } from "@/shared/i18n";
import { buttonClass, ErrorNotice } from "@/shared/ui";
import { lastPlanWasItem, PLANNING, stillCurrent } from "../model/move";

/** Moving one review item the person chose, for the results to place. */
export interface ReviewMove {
  /** Plans `item` alone (which moves nothing) and asks to move it. */
  readonly start: (item: CandidateView) => void;
  /** A plan is being made, or a clean-up or scan runs. */
  readonly busy: boolean;
  /** The item being planned or moved now, by ID, for its row to show. */
  readonly moving: string | null;
  /**
   * Why the chosen item could not be planned or moved, with a retry of that
   * item, for the top of the results; `null` when nothing went wrong.
   */
  readonly error: ReactNode;
}

/**
 * Moves one review item the person chose (ADR-0051): the host plans that
 * item alone, without the policy's proposals, and asks to execute exactly
 * that plan; the host's confirmation window (FS-1) names it as the person's
 * choice, and nothing moves unless they confirm there. A plan made for an
 * earlier scan is never executed.
 */
export function useReviewMove(): ReviewMove {
  const { m } = useI18n();
  const transport = useTransport();
  const session = useScanSession();
  const scan = session.scan?.scan ?? null;
  // The scan shown now, read when the plan comes back.
  const shown = useRef(scan);
  shown.current = scan;
  const [target, setTarget] = useState<string | null>(null);
  const planning = useIsMutating({ mutationKey: PLANNING }) > 0;
  const itemMoved = lastPlanWasItem(
    useMutationState({
      filters: { mutationKey: PLANNING },
      select: (mutation) => mutation.state.variables,
    }),
  );
  const plan = useMutation({
    mutationKey: PLANNING,
    mutationFn: async (request: { readonly id: string; readonly scan: string | null }) => ({
      planned: await transport.plan([request.id]),
      scan: request.scan,
    }),
  });
  const { execute } = session;
  const run = (id: string) => {
    setTarget(id);
    session.resetCleanUp();
    plan.mutate(
      { id, scan },
      {
        onSuccess: ({ planned, scan: planScan }) => {
          if (stillCurrent(planScan, shown.current)) {
            execute(planned);
          }
        },
      },
    );
  };
  const retry =
    target === null ? null : (
      <button type="button" onClick={() => run(target)} className={buttonClass("secondary", "sm")}>
        {m.common.tryAgain}
      </button>
    );
  return {
    start: (item) => run(item.id),
    busy: planning || session.cleaning || session.scanning,
    moving: plan.isPending || session.cleaning ? target : null,
    error: plan.isError ? (
      <ErrorNotice message={m.action.planFailed} error={plan.error}>
        {retry}
      </ErrorNotice>
    ) : itemMoved && session.cleanUpError !== null && !declined(session.cleanUpError) ? (
      <ErrorNotice message={m.confirm.failed} error={session.cleanUpError}>
        {retry}
      </ErrorNotice>
    ) : null,
  };
}
