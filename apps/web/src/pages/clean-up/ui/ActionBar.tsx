import type { PlanView, QuarantineReportView } from "@lumen/api-client";
import { useMutation } from "@tanstack/react-query";
import { useId, useRef, useState } from "react";
import { useTransport } from "@/shared/api";
import { useI18n } from "@/shared/i18n";
import { formatBytes } from "@/shared/lib";
import { buttonClass, ErrorNotice } from "@/shared/ui";
import { ConfirmDialog } from "./ConfirmDialog";

/**
 * The bar along the bottom of the results with the one button that cleans
 * up: it makes a plan of every proposed item (which moves nothing), asks in
 * a dialog, and only on "Move to Quarantine" executes that plan.
 */
export function ActionBar({
  proposed,
  bytes,
  onDone,
}: {
  /** How many items the assessment proposes; none disables the button. */
  readonly proposed: number;
  /** What they would free, as a decimal string. */
  readonly bytes: string;
  readonly onDone: (report: QuarantineReportView) => void;
}) {
  const { m } = useI18n();
  const transport = useTransport();
  const hint = useId();
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
  return (
    <div className="sticky bottom-0 z-10 border-t border-line bg-window/80 backdrop-blur-xl backdrop-saturate-150">
      <div className="mx-auto max-w-[880px] space-y-2 px-4 py-3 min-[720px]:px-10">
        {plan.isError && <ErrorNotice message={m.action.planFailed} error={plan.error} />}
        {execute.isError && !open && (
          <ErrorNotice message={m.confirm.failed} error={execute.error} />
        )}
        <div className="flex items-center gap-3">
          <div className="min-w-0 flex-1">
            <p className="truncate text-[13px] font-semibold tabular-nums text-ink">
              {nothing ? m.action.nothing : m.action.summary(proposed, formatBytes(bytes))}
            </p>
            <p id={hint} className="truncate text-[12px] text-ink-2">
              {nothing ? m.results.nothing : m.action.hint}
            </p>
          </div>
          <button
            ref={button}
            type="button"
            onClick={() => {
              execute.reset();
              plan.mutate();
            }}
            disabled={nothing || plan.isPending || execute.isPending}
            aria-describedby={hint}
            className={buttonClass("primary", "lg")}
          >
            {plan.isPending ? m.action.preparing : m.action.cleanUp}
          </button>
        </div>
      </div>
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
