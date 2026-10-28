import type { HeldItemView } from "@lumen/api-client";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useEffect, useId, useRef, useState } from "react";
import { operationOf, QUERY_KEYS, useTransport } from "@/shared/api";
import { useI18n } from "@/shared/i18n";
import { baseName, formatBytes, parentPath } from "@/shared/lib";
import { buttonClass, ErrorNotice } from "@/shared/ui";
import { daysHeld, type LastAction } from "../lib/outcome";

/**
 * The list's columns. Wide: name, original location, size, time held and
 * the actions on one line. Narrow (below 42rem): name and size, location
 * and time held, then the actions.
 */
export const HELD_GRID =
  "grid grid-cols-[minmax(0,1fr)_auto] gap-x-3 @2xl:grid-cols-[minmax(0,2fr)_minmax(0,3fr)_5.5rem_5.5rem_12.5rem]";

/**
 * One held item: restore it, or (only if it is retained, and only after a
 * second, explicit confirmation) remove it for good.
 */
export function HeldRow({
  item,
  onDone,
  selected,
  onToggle,
  locked,
}: {
  readonly item: HeldItemView;
  readonly onDone: (last: LastAction) => void;
  /** Whether the item is selected for an action on several items. */
  readonly selected: boolean;
  readonly onToggle: () => void;
  /** An action on several items is running: the row's own buttons wait. */
  readonly locked: boolean;
}) {
  const { m } = useI18n();
  const transport = useTransport();
  const queries = useQueryClient();
  const warning = useId();
  const [confirming, setConfirming] = useState(false);
  const ask = useRef<HTMLButtonElement>(null);
  const dismiss = useRef<HTMLButtonElement>(null);
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
      dismiss.current?.focus();
    } else if (asked.current) {
      asked.current = false;
      ask.current?.focus();
    }
  }, [confirming]);

  const busy = locked || restore.isPending || finalize.isPending;
  const retained = item.state === "retained";
  const location = item.original === null ? null : parentPath(item.original);
  const name = item.original === null ? m.held.unknownPath : baseName(item.original);
  return (
    <li className="flex gap-3 py-1.5">
      <input
        type="checkbox"
        checked={selected}
        onChange={onToggle}
        disabled={locked}
        aria-label={m.quarantine.selection.item(name)}
        className="mt-[3px] size-4 shrink-0 accent-accent"
      />
      <div className="min-w-0 flex-1">
        <div className={`${HELD_GRID} items-center gap-y-1`}>
          <span className="truncate text-ink">{name}</span>
          <span className="col-start-2 row-start-1 text-right tabular-nums text-ink @2xl:col-start-3">
            {item.reclaim_bytes === null ? "–" : formatBytes(item.reclaim_bytes)}
          </span>
          <span
            title={location ?? undefined}
            className="row-start-2 truncate text-[12px] text-ink-3 select-text @2xl:col-start-2 @2xl:row-start-1 @2xl:text-[13px]"
          >
            {location}
          </span>
          <span
            title={retained ? undefined : m.held.afterFailure}
            className={`col-start-2 row-start-2 text-right text-[12px] tabular-nums @2xl:col-start-4 @2xl:row-start-1 @2xl:text-left @2xl:text-[13px] ${retained ? "text-ink-2" : "text-amber-700 dark:text-amber-400"}`}
          >
            {retained ? m.held.days(daysHeld(item.since, new Date())) : m.held.restoreOnly}
          </span>
          <div className="col-span-2 flex justify-end gap-1 @2xl:col-span-1 @2xl:col-start-5 @2xl:row-start-1">
            <button
              type="button"
              onClick={() => restore.mutate()}
              disabled={busy}
              className={buttonClass("secondary", "sm")}
            >
              {m.held.restore}
            </button>
            {retained && (
              <button
                ref={ask}
                type="button"
                onClick={() => setConfirming(true)}
                disabled={busy || confirming}
                className={buttonClass("dangerQuiet", "sm")}
              >
                {m.held.removeForGoodAsk}
              </button>
            )}
          </div>
        </div>
        {retained && confirming && (
          <div className="mt-2 mb-1 flex flex-wrap items-center gap-x-4 gap-y-2 rounded-md bg-red-500/8 px-3 py-2.5 dark:bg-red-400/10">
            <p id={warning} className="min-w-0 flex-1 basis-64 text-red-900 dark:text-red-200">
              {m.held.removeForGoodWarning}
            </p>
            <div className="flex gap-2">
              <button
                ref={dismiss}
                type="button"
                onClick={() => setConfirming(false)}
                disabled={busy}
                className={buttonClass("secondary", "sm")}
              >
                {m.common.close}
              </button>
              <button
                type="button"
                onClick={() => finalize.mutate()}
                disabled={busy}
                aria-describedby={warning}
                className={buttonClass("danger", "sm")}
              >
                {m.held.removeForGood}
              </button>
            </div>
          </div>
        )}
        {(restore.isError || finalize.isError) && (
          <div className="mt-2">
            <ErrorNotice message={m.held.failed} error={restore.error ?? finalize.error} />
          </div>
        )}
      </div>
    </li>
  );
}
