import { useIsMutating, useMutation, useMutationState } from "@tanstack/react-query";
import type { ReactNode } from "react";
import { useScanSession } from "@/entities/scan-session";
import { declined, useTransport } from "@/shared/api";
import { useI18n } from "@/shared/i18n";
import { buttonClass, ErrorNotice } from "@/shared/ui";
import { lastPlanWasItem, PLANNING } from "../model/move";

/** The parts of the clean-up action, for the page to place. */
export interface CleanUpAction {
  /** The toolbar's primary button. */
  readonly button: ReactNode;
  /** What went wrong, for the top of the content; `null` when nothing did. */
  readonly error: ReactNode;
}

/**
 * The one action that cleans up: the button makes a plan of every proposed
 * item (which moves nothing) and asks to execute exactly that plan. The
 * desktop host then asks the person in its own confirmation window (FS-1),
 * which lists what moves; nothing moves unless they confirm there, and
 * closing it is no failure. With nothing proposed the button is disabled
 * and its tooltip says why.
 */
export function useCleanUpAction({
  proposed,
  onConfirm,
}: {
  /** How many items the assessment proposes. */
  readonly proposed: number;
  readonly onConfirm: () => void;
}): CleanUpAction {
  const { m } = useI18n();
  const transport = useTransport();
  const session = useScanSession();
  const planning = useIsMutating({ mutationKey: PLANNING }) > 0;
  // A failed move of one chosen item is the item's to show and retry.
  const itemMoved = lastPlanWasItem(
    useMutationState({
      filters: { mutationKey: PLANNING },
      select: (mutation) => mutation.state.variables,
    }),
  );
  const plan = useMutation({
    mutationKey: PLANNING,
    mutationFn: () => transport.plan(),
    onSuccess: (planned) => {
      onConfirm();
      session.execute(planned);
    },
  });
  const nothing = proposed === 0;
  const prepare = () => {
    session.resetCleanUp();
    plan.mutate();
  };
  const retry = (
    <button type="button" onClick={prepare} className={buttonClass("secondary", "sm")}>
      {m.common.tryAgain}
    </button>
  );
  return {
    button: (
      <button
        type="button"
        onClick={prepare}
        disabled={nothing || planning || session.cleaning}
        title={nothing ? m.action.nothing : undefined}
        className={buttonClass("primary")}
      >
        {session.cleaning
          ? m.confirm.moving
          : plan.isPending
            ? m.action.preparing
            : m.action.cleanUp}
      </button>
    ),
    error: plan.isError ? (
      <ErrorNotice message={m.action.planFailed} error={plan.error}>
        {retry}
      </ErrorNotice>
    ) : session.cleanUpError !== null && !declined(session.cleanUpError) && !itemMoved ? (
      <ErrorNotice message={m.confirm.failed} error={session.cleanUpError}>
        {retry}
      </ErrorNotice>
    ) : null,
  };
}
