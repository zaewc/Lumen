import type { HeldItemView } from "@lumen/api-client";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { operationOf, useTransport } from "@/shared/api";
import { daysHeld, finalizeText, restoreText } from "./outcome";

/**
 * One held item: restore it, or (only if it is retained, and only after a
 * second, explicit confirmation) remove it for good.
 */
export function HeldItem({ item }: { readonly item: HeldItemView }) {
  const transport = useTransport();
  const queries = useQueryClient();
  const [confirming, setConfirming] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const refresh = () => queries.invalidateQueries({ queryKey: ["held"] });
  const restore = useMutation({
    mutationFn: () => transport.restore(operationOf(item)),
    onSuccess: (done) => {
      setMessage(restoreText(done.outcome));
      void refresh();
    },
  });
  const finalize = useMutation({
    mutationFn: () => transport.finalize(operationOf(item)),
    onSuccess: (done) => {
      setConfirming(false);
      setMessage(finalizeText(done.outcome));
      void refresh();
    },
  });
  const busy = restore.isPending || finalize.isPending;
  const retained = item.state === "retained";
  return (
    <li className="space-y-1 rounded border border-neutral-200 p-3 text-sm">
      <div className="font-mono break-all">{item.original ?? "(unknown path)"}</div>
      <div className="text-neutral-600">
        {retained ? "Retained" : "Held after a failed move"} · {daysHeld(item.since, new Date())}{" "}
        day(s)
      </div>
      <div className="flex flex-wrap gap-2 pt-1">
        <button
          type="button"
          onClick={() => restore.mutate()}
          disabled={busy}
          className="rounded border border-neutral-400 px-2 py-1 disabled:opacity-40"
        >
          Restore
        </button>
        {retained && !confirming && (
          <button
            type="button"
            onClick={() => setConfirming(true)}
            disabled={busy}
            className="rounded border border-red-300 px-2 py-1 text-red-800 disabled:opacity-40"
          >
            Remove for good…
          </button>
        )}
        {retained && confirming && (
          <>
            <span className="self-center text-red-800">
              This cannot be undone. It is checked for use first.
            </span>
            <button
              type="button"
              onClick={() => finalize.mutate()}
              disabled={busy}
              className="rounded bg-red-700 px-2 py-1 text-white disabled:opacity-40"
            >
              Remove for good
            </button>
            <button
              type="button"
              onClick={() => setConfirming(false)}
              disabled={busy}
              className="rounded border border-neutral-400 px-2 py-1"
            >
              Cancel
            </button>
          </>
        )}
      </div>
      {message && <p className="text-neutral-800">{message}</p>}
      {(restore.isError || finalize.isError) && (
        <p className="text-red-700">{String(restore.error ?? finalize.error)}</p>
      )}
    </li>
  );
}
