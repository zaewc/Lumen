import type { HeldItemView } from "@lumen/api-client";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useEffect, useId, useRef, useState } from "react";
import { operationOf, useTransport } from "@/shared/api";
import { useI18n } from "@/shared/i18n";
import { baseName, formatBytes } from "@/shared/lib";
import { buttonClass, ErrorNotice, InfoTip } from "@/shared/ui";
import { daysHeld, type LastAction } from "../lib/outcome";

/**
 * One held item: restore it, or (only if it is retained, and only after a
 * second, explicit confirmation) remove it for good.
 */
export function HeldRow({
  item,
  onDone,
}: {
  readonly item: HeldItemView;
  readonly onDone: (last: LastAction) => void;
}) {
  const { m } = useI18n();
  const transport = useTransport();
  const queries = useQueryClient();
  const warning = useId();
  const [confirming, setConfirming] = useState(false);
  const ask = useRef<HTMLButtonElement>(null);
  const cancel = useRef<HTMLButtonElement>(null);
  const asked = useRef(false);
  const refresh = () => queries.invalidateQueries({ queryKey: ["held"] });
  const restore = useMutation({
    mutationFn: () => transport.restore(operationOf(item)),
    onSuccess: (restored) => {
      onDone({ original: item.original, done: { action: "restore", outcome: restored.outcome } });
      void refresh();
    },
  });
  const finalize = useMutation({
    mutationFn: () => transport.finalize(operationOf(item)),
    onSuccess: (finalized) => {
      setConfirming(false);
      onDone({ original: item.original, done: { action: "finalize", outcome: finalized.outcome } });
      void refresh();
    },
  });

  // Focus moves into the confirmation when it opens, and back when it closes.
  useEffect(() => {
    if (confirming) {
      asked.current = true;
      cancel.current?.focus();
    } else if (asked.current) {
      asked.current = false;
      ask.current?.focus();
    }
  }, [confirming]);

  const busy = restore.isPending || finalize.isPending;
  const retained = item.state === "retained";
  return (
    <li className="py-4">
      <div className="flex flex-col gap-3 sm:flex-row sm:items-start sm:justify-between">
        <div className="min-w-0">
          <p className="font-semibold break-words text-neutral-900">
            {item.original === null ? m.held.unknownPath : baseName(item.original)}
            {item.reclaim_bytes !== null && (
              <span className="ml-2 font-normal text-neutral-600">
                {formatBytes(item.reclaim_bytes)}
              </span>
            )}
          </p>
          {item.original !== null && (
            <p className="mt-0.5 font-mono text-xs break-all text-neutral-500">{item.original}</p>
          )}
          <p className="mt-1 text-sm text-neutral-600">
            {retained ? m.held.days(daysHeld(item.since, new Date())) : m.held.afterFailure}
          </p>
        </div>
        <div className="flex shrink-0 flex-wrap gap-2">
          <button
            type="button"
            onClick={() => restore.mutate()}
            disabled={busy}
            className={buttonClass("secondary", "sm")}
          >
            {m.held.restore}
          </button>
          {retained && !confirming && (
            <button
              ref={ask}
              type="button"
              onClick={() => setConfirming(true)}
              disabled={busy}
              className={buttonClass("dangerQuiet", "sm")}
            >
              {m.held.removeForGoodAsk}
            </button>
          )}
        </div>
      </div>
      {retained && confirming && (
        <div className="mt-3 rounded-xl border border-red-200 bg-red-50 p-4">
          <div className="flex items-start gap-2">
            <p id={warning} className="flex-1 text-sm font-medium text-red-900">
              {m.held.removeForGoodWarning}
            </p>
            <InfoTip term="finalize" />
          </div>
          <div className="mt-3 flex flex-wrap gap-2">
            <button
              type="button"
              onClick={() => finalize.mutate()}
              disabled={busy}
              aria-describedby={warning}
              className={buttonClass("danger", "sm")}
            >
              {m.held.removeForGood}
            </button>
            <button
              ref={cancel}
              type="button"
              onClick={() => setConfirming(false)}
              disabled={busy}
              className={buttonClass("secondary", "sm")}
            >
              {m.common.cancel}
            </button>
          </div>
        </div>
      )}
      {(restore.isError || finalize.isError) && (
        <div className="mt-3">
          <ErrorNotice message={m.held.failed} error={restore.error ?? finalize.error} />
        </div>
      )}
    </li>
  );
}
