import { useMutation } from "@tanstack/react-query";
import { type ReactNode, useEffect, useRef, useState } from "react";
import { useScanSession } from "@/entities/scan-session";
import { useTransport } from "@/shared/api";
import { useI18n } from "@/shared/i18n";
import { buttonClass, ErrorNotice } from "@/shared/ui";
import { ConfirmDialog } from "./ConfirmDialog";

/** The parts of the clean-up action, for the page to place. */
export interface CleanUpAction {
  /** The toolbar's primary button. */
  readonly button: ReactNode;
  /** What went wrong, for the top of the content; `null` when nothing did. */
  readonly error: ReactNode;
  /** The confirmation dialog. */
  readonly dialog: ReactNode;
}

/**
 * The one action that cleans up: the button makes a plan of every proposed
 * item (which moves nothing), the dialog asks, and only "Move to
 * Quarantine" executes exactly that plan. With nothing proposed the button
 * is disabled and its tooltip says why.
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
  const button = useRef<HTMLButtonElement>(null);
  const [open, setOpen] = useState(false);
  const plan = useMutation({
    mutationFn: () => transport.plan(),
    onSuccess: () => setOpen(true),
  });
  useEffect(() => {
    if (session.report !== null) {
      setOpen(false);
    }
  }, [session.report]);
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
        ref={button}
        type="button"
        onClick={prepare}
        disabled={nothing || plan.isPending || session.cleaning}
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
    ) : session.cleanUpError !== null && !open ? (
      <ErrorNotice message={m.confirm.failed} error={session.cleanUpError}>
        {retry}
      </ErrorNotice>
    ) : null,
    dialog: plan.data ? (
      <ConfirmDialog
        open={open}
        plan={plan.data}
        busy={session.cleaning}
        error={session.cleanUpError}
        onConfirm={() => {
          if (plan.data) {
            onConfirm();
            session.execute(plan.data);
          }
        }}
        onClose={() => setOpen(false)}
        returnFocus={button}
      />
    ) : null,
  };
}
