import type { CandidateView } from "@lumen/api-client";
import { useMutation } from "@tanstack/react-query";
import { type ReactNode, useRef } from "react";
import { useScanSession } from "@/entities/scan-session";
import { useTransport } from "@/shared/api";
import { useI18n } from "@/shared/i18n";
import { ErrorNotice } from "@/shared/ui";

/** Moving one review item the person chose, for the results to place. */
export interface ReviewMove {
  /** Plans `item` alone (which moves nothing) and asks to move it. */
  readonly start: (item: CandidateView) => void;
  /** A plan is being made, or a clean-up or scan runs. */
  readonly busy: boolean;
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
  const plan = useMutation({
    mutationFn: async (request: { readonly id: string; readonly scan: string | null }) => ({
      planned: await transport.plan([request.id]),
      scan: request.scan,
    }),
  });
  const { execute } = session;
  return {
    start: (item) => {
      session.resetCleanUp();
      plan.mutate(
        { id: item.id, scan },
        {
          onSuccess: ({ planned, scan: planScan }) => {
            if (planScan === shown.current) {
              execute(planned);
            }
          },
        },
      );
    },
    busy: plan.isPending || session.cleaning || session.scanning,
    error: plan.isError ? <ErrorNotice message={m.action.planFailed} error={plan.error} /> : null,
  };
}
