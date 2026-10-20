import type { PlanView, QuarantineReportView } from "@lumen/api-client";
import { useMutation } from "@tanstack/react-query";
import { useRef, useState } from "react";
import { useTransport } from "@/shared/api";
import { useI18n } from "@/shared/i18n";
import { buttonClass, ErrorNotice } from "@/shared/ui";
import { ConfirmDialog } from "./ConfirmDialog";

/**
 * "Clean up": makes a plan of every proposed item (which moves nothing),
 * asks in a dialog, and only on "Move to Quarantine" executes that plan.
 */
export function CleanUpAction({
  proposed,
  onDone,
}: {
  /** How many items the assessment proposes; none disables the button. */
  readonly proposed: number;
  readonly onDone: (report: QuarantineReportView) => void;
}) {
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
  return (
    <div className="space-y-3">
      <button
        ref={button}
        type="button"
        onClick={() => {
          execute.reset();
          plan.mutate();
        }}
        disabled={proposed === 0 || plan.isPending || execute.isPending}
        className={`${buttonClass("primary", "lg")} w-full sm:w-auto`}
      >
        {plan.isPending ? m.results.preparing : m.results.cleanUp}
      </button>
      {proposed === 0 && (
        <p className="text-sm text-neutral-600 dark:text-neutral-400">{m.results.nothing}</p>
      )}
      {plan.isError && <ErrorNotice message={m.results.planFailed} error={plan.error} />}
      {execute.isError && !open && <ErrorNotice message={m.confirm.failed} error={execute.error} />}
      {plan.data && (
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
      )}
    </div>
  );
}
