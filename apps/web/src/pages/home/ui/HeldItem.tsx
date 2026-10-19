import type { FinalizeView, HeldItemView, RestoreView } from "@lumen/api-client";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { operationOf, useTransport } from "@/shared/api";
import { type Messages, useI18n } from "@/shared/i18n";
import { InfoTip } from "@/shared/ui";
import { daysHeld, finalizeText, restoreText } from "./outcome";

/** The last action's outcome, kept as data so it follows a language switch. */
type Done =
  | { readonly action: "restore"; readonly outcome: RestoreView }
  | { readonly action: "finalize"; readonly outcome: FinalizeView };

function doneText(m: Messages, done: Done): string {
  switch (done.action) {
    case "restore":
      return restoreText(m, done.outcome);
    case "finalize":
      return finalizeText(m, done.outcome);
  }
}

/**
 * One held item: restore it, or (only if it is retained, and only after a
 * second, explicit confirmation) remove it for good.
 */
export function HeldItem({ item }: { readonly item: HeldItemView }) {
  const { m } = useI18n();
  const transport = useTransport();
  const queries = useQueryClient();
  const [confirming, setConfirming] = useState(false);
  const [done, setDone] = useState<Done | null>(null);
  const refresh = () => queries.invalidateQueries({ queryKey: ["held"] });
  const restore = useMutation({
    mutationFn: () => transport.restore(operationOf(item)),
    onSuccess: (restored) => {
      setDone({ action: "restore", outcome: restored.outcome });
      void refresh();
    },
  });
  const finalize = useMutation({
    mutationFn: () => transport.finalize(operationOf(item)),
    onSuccess: (finalized) => {
      setConfirming(false);
      setDone({ action: "finalize", outcome: finalized.outcome });
      void refresh();
    },
  });
  const busy = restore.isPending || finalize.isPending;
  const retained = item.state === "retained";
  return (
    <li className="space-y-1 rounded border border-neutral-200 p-3 text-sm">
      <div className="font-mono break-all">{item.original ?? m.held.unknownPath}</div>
      <div className="flex items-center gap-2 text-neutral-600">
        <span>
          {retained ? m.held.retained : m.held.heldAfterFailure} ·{" "}
          {m.held.age(daysHeld(item.since, new Date()))}
        </span>
        <InfoTip term="held" />
      </div>
      <div className="flex flex-wrap items-center gap-2 pt-1">
        <button
          type="button"
          onClick={() => restore.mutate()}
          disabled={busy}
          className="rounded border border-neutral-400 px-2 py-1 disabled:opacity-40"
        >
          {m.held.restore}
        </button>
        <InfoTip term="restore" />
        {retained && !confirming && (
          <button
            type="button"
            onClick={() => setConfirming(true)}
            disabled={busy}
            className="rounded border border-red-300 px-2 py-1 text-red-800 disabled:opacity-40"
          >
            {m.held.removeForGoodAsk}
          </button>
        )}
        {retained && !confirming && <InfoTip term="finalize" />}
        {retained && confirming && (
          <>
            <span className="self-center text-red-800">{m.held.removeForGoodWarning}</span>
            <InfoTip term="finalize" />
            <button
              type="button"
              onClick={() => finalize.mutate()}
              disabled={busy}
              className="rounded bg-red-700 px-2 py-1 text-white disabled:opacity-40"
            >
              {m.held.removeForGood}
            </button>
            <button
              type="button"
              onClick={() => setConfirming(false)}
              disabled={busy}
              className="rounded border border-neutral-400 px-2 py-1"
            >
              {m.held.cancel}
            </button>
          </>
        )}
      </div>
      {done && <p className="text-neutral-800">{doneText(m, done)}</p>}
      {(restore.isError || finalize.isError) && (
        <p className="text-red-700">{String(restore.error ?? finalize.error)}</p>
      )}
    </li>
  );
}
