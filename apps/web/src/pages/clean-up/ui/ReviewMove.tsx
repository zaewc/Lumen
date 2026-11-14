import type { CandidateView } from "@lumen/api-client";
import { useIsMutating, useMutation } from "@tanstack/react-query";
import { type ReactNode, useRef, useState } from "react";
import { useScanSession } from "@/entities/scan-session";
import { useTransport } from "@/shared/api";
import { useI18n } from "@/shared/i18n";
import { ErrorNotice } from "@/shared/ui";
import { PLANNING, stillCurrent } from "../model/move";

/** Moving one review item the person chose, for the results to place. */
export interface ReviewMove {
  /** Plans `item` alone (which moves nothing) and asks to move it. */
  readonly start: (item: CandidateView) => void;
  /** A plan is being made, or a clean-up or scan runs. */
  readonly busy: boolean;
  /** The item being planned or moved now, by ID, for its row to show. */
  readonly moving: string | null;
  /**
   * Why the item could not be planned, for the top of the results; `null`
   * when nothing went wrong. A failed move shows with the page's clean-up.
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
  const plan = useMutation({
    mutationKey: PLANNING,
    mutationFn: async (request: { readonly id: string; readonly scan: string | null }) => ({
      planned: await transport.plan([request.id]),
      scan: request.scan,
    }),
  });
  const { execute } = session;
  return {
    start: (item) => {
      setTarget(item.id);
      session.resetCleanUp();
      plan.mutate(
        { id: item.id, scan },
        {
          onSuccess: ({ planned, scan: planScan }) => {
            if (stillCurrent(planScan, shown.current)) {
              execute(planned);
            }
          },
        },
      );
    },
    busy: planning || session.cleaning || session.scanning,
    moving: plan.isPending || session.cleaning ? target : null,
    error: plan.isError ? <ErrorNotice message={m.action.planFailed} error={plan.error} /> : null,
  };
}
