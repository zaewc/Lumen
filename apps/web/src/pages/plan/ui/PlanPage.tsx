import type { PlanView, QuarantineReportView } from "@lumen/api-client";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { CandidateList } from "@/entities/candidate";
import { useTransport } from "@/shared/api";
import { formatBytes } from "@/shared/lib";
import { outcomeText } from "./outcome";

function total(plan: PlanView): string {
  return plan.items.reduce((sum, item) => sum + BigInt(item.reclaimable_bytes), 0n).toString();
}

/**
 * Make a plan from the latest assessment, read it, confirm it explicitly, and
 * quarantine exactly that plan (ADR-0034). Everything moved stays restorable.
 */
export function PlanPage() {
  const transport = useTransport();
  const queries = useQueryClient();
  const [plan, setPlan] = useState<PlanView | null>(null);
  const [understood, setUnderstood] = useState(false);
  const [report, setReport] = useState<QuarantineReportView | null>(null);
  const make = useMutation({
    mutationFn: () => transport.plan(),
    onSuccess: (made) => {
      setPlan(made);
      setUnderstood(false);
      setReport(null);
    },
  });
  const execute = useMutation({
    mutationFn: (confirmed: PlanView) =>
      transport.execute({ plan: confirmed.plan, confirmation: confirmed.hash }),
    onSuccess: (done) => {
      setReport(done);
      void queries.invalidateQueries({ queryKey: ["held"] });
    },
  });
  return (
    <main className="mx-auto max-w-3xl space-y-8 p-8">
      <h1 className="text-2xl font-semibold">Plan</h1>
      <section className="space-y-2">
        <p className="text-sm">
          A plan holds every item the policy proposes for quarantine in the latest scan. Making it
          moves nothing.
        </p>
        <button
          type="button"
          onClick={() => make.mutate()}
          disabled={make.isPending || execute.isPending}
          className="rounded bg-neutral-900 px-3 py-1.5 text-sm text-white disabled:opacity-40"
        >
          {make.isPending ? "Planning…" : "Make a plan"}
        </button>
        {make.isError && <p className="text-sm text-red-700">{String(make.error)}</p>}
      </section>
      {plan && report === null && (
        <section className="space-y-4">
          <h2 className="text-lg font-medium">
            {plan.items.length} item(s), {formatBytes(total(plan))}
          </h2>
          <p className="text-xs text-neutral-600">
            Made {plan.created_at}; valid for 24 hours. Plan hash{" "}
            <span className="font-mono break-all">{plan.hash}</span>
          </p>
          <CandidateList candidates={plan.items} />
          <label className="flex items-start gap-2 text-sm">
            <input
              type="checkbox"
              checked={understood}
              onChange={(event) => setUnderstood(event.target.checked)}
            />
            <span>
              Move exactly these items into quarantine. Each is checked again first, and anything
              that changed stays where it is. Nothing is deleted: every item can be restored until I
              remove it for good.
            </span>
          </label>
          <button
            type="button"
            onClick={() => execute.mutate(plan)}
            disabled={!understood || execute.isPending}
            className="rounded bg-amber-700 px-3 py-1.5 text-sm text-white disabled:opacity-40"
          >
            {execute.isPending ? "Quarantining…" : `Quarantine ${plan.items.length} item(s)`}
          </button>
          {execute.isError && <p className="text-sm text-red-700">{String(execute.error)}</p>}
        </section>
      )}
      {report && (
        <section className="space-y-3">
          <h2 className="text-lg font-medium">
            {formatBytes(report.moved_bytes)} now in quarantine
          </h2>
          <p className="text-xs text-neutral-600">
            Space is freed only when items are removed for good.
          </p>
          {report.warnings.map((warning) => (
            <p key={warning} className="text-sm text-amber-800">
              {warning}
            </p>
          ))}
          <ul className="space-y-2">
            {report.items.map((item) => (
              <li key={item.path} className="text-sm">
                <span className="font-mono break-all">{item.path}</span>
                <p className="text-neutral-700">{outcomeText(item.result)}</p>
              </li>
            ))}
          </ul>
        </section>
      )}
    </main>
  );
}
