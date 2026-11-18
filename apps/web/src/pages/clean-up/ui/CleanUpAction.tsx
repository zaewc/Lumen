import { useIsMutating, useMutation } from "@tanstack/react-query";
import type { ReactNode } from "react";
import { useScanSession } from "@/entities/scan-session";
import { declined, useTransport } from "@/shared/api";
import { useI18n } from "@/shared/i18n";
import { formatBytes } from "@/shared/lib";
import { buttonClass, ErrorNotice } from "@/shared/ui";
import { PLANNING, useLatestPlan } from "../model/move";
import type { Selected } from "../model/selection";

/** The parts of the clean-up action, for the page to place. */
export interface CleanUpAction {
  /** The toolbar's primary button. */
  readonly button: ReactNode;
  /** What went wrong, for the top of the content; `null` when nothing did. */
  readonly error: ReactNode;
}

/**
 * The one action that cleans up: the button names what is checked and makes
 * a plan of exactly that (ADR-0069): the proposals not left out and the
 * chosen review items. Planning moves nothing; it asks to execute that plan. The
 * desktop host then asks the person in its own confirmation window (FS-1),
 * which lists what moves; nothing moves unless they confirm there, and
 * closing it is no failure. With nothing checked the button is disabled
 * and its tooltip says why.
 */
export function useCleanUpAction({
  selected,
  onConfirm,
}: {
  /** What is checked. */
  readonly selected: Selected;
  readonly onConfirm: () => void;
}): CleanUpAction {
  const { m } = useI18n();
  const transport = useTransport();
  const session = useScanSession();
  const planning = useIsMutating({ mutationKey: PLANNING }) > 0;
  // Only the toolbar's own latest plan is its to show and retry; one chosen
  // item's is the item's.
  const latest = useLatestPlan();
  const mine = latest !== null && latest.item === null;
  const plan = useMutation({
    mutationKey: PLANNING,
    // Only apps checked: no clean-up plan, the apps alone.
    mutationFn: () =>
      selected.items > 0
        ? transport.plan(selected.chosen, selected.leftOut)
        : Promise.resolve(null),
    onSuccess: (planned) => {
      onConfirm();
      session.execute(planned, selected.apps, selected.copies);
    },
  });
  const nothing = selected.count === 0;
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
        title={nothing ? m.action.nothingSelected : undefined}
        className={buttonClass("primary")}
      >
        {session.cleaning
          ? m.confirm.moving
          : plan.isPending
            ? m.action.preparing
            : nothing
              ? m.action.cleanUp
              : m.action.cleanUpSelected(selected.count, formatBytes(selected.bytes))}
      </button>
    ),
    error:
      mine && latest.failed ? (
        <ErrorNotice message={m.action.planFailed} error={latest.error}>
          {retry}
        </ErrorNotice>
      ) : mine && session.cleanUpError !== null && !declined(session.cleanUpError) ? (
        <ErrorNotice message={m.confirm.failed} error={session.cleanUpError}>
          {retry}
        </ErrorNotice>
      ) : null,
  };
}
