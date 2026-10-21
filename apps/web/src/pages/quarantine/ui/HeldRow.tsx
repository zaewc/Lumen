import type { HeldItemView } from "@lumen/api-client";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useEffect, useId, useRef, useState } from "react";
import { operationOf, QUERY_KEYS, useTransport } from "@/shared/api";
import { useI18n } from "@/shared/i18n";
import { baseName, formatBytes, parentPath } from "@/shared/lib";
import { buttonClass, ErrorNotice, FolderIcon, InfoTip, RestoreIcon } from "@/shared/ui";
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
  const refresh = async () => {
    await queries.invalidateQueries({ queryKey: QUERY_KEYS.held });
    await queries.invalidateQueries({ queryKey: QUERY_KEYS.disk });
  };
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
    <li className="px-4 py-3.5">
      <div className="flex flex-wrap items-center gap-x-3 gap-y-2.5">
        <span
          aria-hidden="true"
          className="flex size-8 shrink-0 items-center justify-center rounded-lg bg-fill-strong text-ink-2"
        >
          <FolderIcon className="size-[18px]" />
        </span>
        <div className="min-w-0 flex-1 basis-40">
          <p className="truncate text-[14px] font-medium text-ink">
            {item.original === null ? m.held.unknownPath : baseName(item.original)}
          </p>
          {item.original !== null && (
            <p
              className="mt-0.5 truncate font-mono text-[11px] text-ink-3 select-text"
              title={item.original}
            >
              {parentPath(item.original)}
            </p>
          )}
          <p className="mt-0.5 text-[12px] text-ink-2">
            {item.reclaim_bytes !== null && (
              <span className="font-medium tabular-nums text-ink">
                {formatBytes(item.reclaim_bytes)}
                {" · "}
              </span>
            )}
            {retained ? m.held.days(daysHeld(item.since, new Date())) : m.held.afterFailure}
          </p>
        </div>
        <div className="ml-auto flex shrink-0 gap-1.5">
          <button
            type="button"
            onClick={() => restore.mutate()}
            disabled={busy}
            className={buttonClass("secondary", "sm")}
          >
            <RestoreIcon className="size-4" />
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
        <div className="mt-3 rounded-lg border border-red-500/30 bg-red-500/8 p-3.5 dark:border-red-400/25 dark:bg-red-400/10">
          <div className="flex items-start gap-2">
            <p
              id={warning}
              className="flex-1 text-[13px] font-medium text-red-900 dark:text-red-200"
            >
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
