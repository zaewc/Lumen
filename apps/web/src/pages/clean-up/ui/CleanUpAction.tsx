import type { PlanView, QuarantineReportView } from "@lumen/api-client";
import { useMutation } from "@tanstack/react-query";
import { type ReactNode, useRef, useState } from "react";
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
  onDone,
}: {
  /** How many items the assessment proposes. */
  readonly proposed: number;
  readonly onDone: (report: QuarantineReportView) => void;
}): CleanUpAction {
  const { m } = useI18n();
  const transport = useTransport();
  const button = useRef<HTMLButtonElement>(null);
  const [open, setOpen] = useState(false);
  const plan = useMutation({
    mutationFn: () => transport.plan(),
    onSuccess: () => setOpen(true),
  });
  const execute = useMutation({
    mutationFn: (confirmed: PlanView) =>
      transport.execute({ plan: confirmed.plan, confirmation: confirmed.hash }),
    onSuccess: (report) => {
      setOpen(false);
      onDone(report);
    },
  });
  const nothing = proposed === 0;
  const prepare = () => {
    execute.reset();
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
        disabled={nothing || plan.isPending || execute.isPending}
        title={nothing ? m.action.nothing : undefined}
        className={buttonClass("primary")}
      >
        {plan.isPending ? m.action.preparing : m.action.cleanUp}
      </button>
    ),
    error: plan.isError ? (
      <ErrorNotice message={m.action.planFailed} error={plan.error}>
        {retry}
      </ErrorNotice>
    ) : execute.isError && !open ? (
      <ErrorNotice message={m.confirm.failed} error={execute.error}>
        {retry}
      </ErrorNotice>
    ) : null,
    dialog: plan.data ? (
      <ConfirmDialog
        open={open}
        plan={plan.data}
        busy={execute.isPending}
        error={execute.isError ? execute.error : null}
        onConfirm={() => {
          if (plan.data) {
            execute.mutate(plan.data);
          }
        }}
        onClose={() => setOpen(false)}
        returnFocus={button}
      />
    ) : null,
  };
}
