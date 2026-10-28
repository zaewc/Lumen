import { useMutation, useQueryClient } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { useEffect, useId, useRef, useState } from "react";
import { heldTotal, useHeld } from "@/entities/held";
import { operationOf, QUERY_KEYS, useTransport } from "@/shared/api";
import { useI18n } from "@/shared/i18n";
import { baseName, formatBytes } from "@/shared/lib";
import { buttonClass, ErrorNotice, InfoTip, Notice, Page } from "@/shared/ui";
import { type Done, doneText, doneWell, type LastAction } from "../lib/outcome";
import {
  actionable,
  type BulkResult,
  bulkResult,
  bulkText,
  chosen,
  type Selection,
  selectionState,
  stillHeld,
  toggled,
} from "../lib/selection";
import { HELD_GRID, HeldRow } from "./HeldRow";

/** What the notice above the list reports: one item's action, or several. */
type Reported = { readonly one: LastAction } | { readonly many: BulkResult };

/**
 * Like a trash can: what quarantine holds, to restore or remove for good,
 * one at a time or several selected together.
 */
export function QuarantinePage() {
  const { m } = useI18n();
  const transport = useTransport();
  const queries = useQueryClient();
  const held = useHeld();
  const warning = useId();
  // Shown above the list, so it stays after a restored item leaves the list.
  const [reported, setReported] = useState<Reported | null>(null);
  const [picked, setPicked] = useState<Selection>(new Set());
  const [confirming, setConfirming] = useState(false);
  const [progress, setProgress] = useState<{ done: number; total: number } | null>(null);
  const items = held.data ?? [];
  // Items that left the list (restored, removed) leave the selection too.
  const selected = stillHeld(items, picked);
  const selectedItems = chosen(items, selected);
  const state = selectionState(items, selected);
  const total = heldTotal(items);
  const size = formatBytes(total.bytes);
  const selectedTotal = heldTotal(selectedItems);
  const selectedSize = formatBytes(selectedTotal.bytes);
  const removable = actionable("finalize", selectedItems);

  const all = useRef<HTMLInputElement>(null);
  useEffect(() => {
    if (all.current) {
      all.current.indeterminate = state === "some";
    }
  }, [state]);

  const bulk = useMutation({
    mutationFn: async (action: Done["action"]) => {
      const { act } = actionable(action, selectedItems);
      const done: Done[] = [];
      for (const [index, item] of act.entries()) {
        setProgress({ done: index, total: act.length });
        if (action === "restore") {
          const restored = await transport.restore(operationOf(item));
          done.push({ action, outcome: restored.outcome });
        } else {
          const finalized = await transport.finalize(operationOf(item));
          done.push({ action, outcome: finalized.outcome });
        }
      }
      return bulkResult(action, done);
    },
    onSuccess: (result) => {
      setReported({ many: result });
      setPicked(new Set());
    },
    onSettled: async () => {
      setProgress(null);
      setConfirming(false);
      await queries.invalidateQueries({ queryKey: QUERY_KEYS.held });
      await queries.invalidateQueries({ queryKey: QUERY_KEYS.disk });
    },
  });
  const working = bulk.isPending;

  const actions =
    selectedItems.length > 0 ? (
      <>
        <span className="text-[12px] tabular-nums text-ink-2">
          {progress
            ? m.quarantine.selection.working(progress.done, progress.total)
            : m.quarantine.selection.count(
                selectedItems.length,
                selectedTotal.exact ? selectedSize : m.quarantine.atLeast(selectedSize),
              )}
        </span>
        <button
          type="button"
          onClick={() => setPicked(new Set())}
          disabled={working}
          className={buttonClass("quiet", "sm")}
        >
          {m.quarantine.selection.clear}
        </button>
        <button
          type="button"
          onClick={() => bulk.mutate("restore")}
          disabled={working}
          className={buttonClass("secondary", "sm")}
        >
          {m.quarantine.selection.restore}
        </button>
        {removable.act.length > 0 && (
          <button
            type="button"
            onClick={() => setConfirming(true)}
            disabled={working || confirming}
            className={buttonClass("dangerQuiet", "sm")}
          >
            {m.quarantine.selection.removeAsk}
          </button>
        )}
      </>
    ) : undefined;

  return (
    <Page
      title={m.quarantine.title}
      accessory={
        <>
          <InfoTip term="quarantine" />
          {items.length > 0 && (
            <span className="tabular-nums">
              {m.common.items(items.length)} · {total.exact ? size : m.quarantine.atLeast(size)}
            </span>
          )}
        </>
      }
      actions={actions}
    >
      <div role="status" className="empty:hidden">
        {reported !== null && "one" in reported && (
          <Notice tone={doneWell(reported.one.done) ? "success" : "warning"}>
            <p>
              {reported.one.original !== null && (
                <span className="font-medium">{baseName(reported.one.original)}: </span>
              )}
              {doneText(m, reported.one.done)}
            </p>
          </Notice>
        )}
        {reported !== null && "many" in reported && (
          <Notice tone={reported.many.well === reported.many.total ? "success" : "warning"}>
            <p>{bulkText(m, reported.many)}</p>
          </Notice>
        )}
      </div>
      {confirming && removable.act.length > 0 && (
        <div className="flex flex-wrap items-center gap-x-4 gap-y-2 rounded-md bg-red-500/8 px-3 py-2.5 dark:bg-red-400/10">
          <div id={warning} className="min-w-0 flex-1 basis-64 text-red-900 dark:text-red-200">
            <p>{m.quarantine.selection.removeWarning(removable.act.length)}</p>
            {removable.skipped > 0 && (
              <p>{m.quarantine.selection.removeSkips(removable.skipped)}</p>
            )}
          </div>
          <div className="flex gap-2">
            <button
              type="button"
              onClick={() => setConfirming(false)}
              disabled={working}
              className={buttonClass("secondary", "sm")}
            >
              {m.common.close}
            </button>
            <button
              type="button"
              onClick={() => bulk.mutate("finalize")}
              disabled={working}
              aria-describedby={warning}
              className={buttonClass("danger", "sm")}
            >
              {m.quarantine.selection.remove}
            </button>
          </div>
        </div>
      )}
      {bulk.isError && <ErrorNotice message={m.held.failed} error={bulk.error} />}
      {held.isPending && <p className="text-ink-3">{m.common.loading}</p>}
      {held.isError && (
        <ErrorNotice message={m.quarantine.loadFailed} error={held.error}>
          <button
            type="button"
            onClick={() => void held.refetch()}
            className={buttonClass("secondary", "sm")}
          >
            {m.common.tryAgain}
          </button>
        </ErrorNotice>
      )}
      {held.isSuccess && items.length === 0 && (
        <section className="space-y-1">
          <h2 className="font-semibold text-ink">{m.quarantine.empty}</h2>
          <p className="text-ink-2">{m.quarantine.emptyHint}</p>
          <Link to="/clean" className="inline-block pt-1 text-accent-ink hover:underline">
            {m.quarantine.goCleanUp}
          </Link>
        </section>
      )}
      {items.length > 0 && (
        <div className="@container">
          <div className="flex items-end gap-3 border-b border-line pb-1">
            <input
              ref={all}
              type="checkbox"
              checked={state === "all"}
              onChange={() =>
                setPicked(state === "all" ? new Set() : new Set(items.map((i) => i.operation)))
              }
              disabled={working}
              aria-label={m.quarantine.selection.all}
              title={m.quarantine.selection.all}
              className="mb-[1px] size-4 shrink-0 accent-accent"
            />
            <div
              aria-hidden="true"
              className={`${HELD_GRID} min-w-0 flex-1 text-[12px] font-medium text-ink-3`}
            >
              <span className="@2xl:hidden">{m.quarantine.selection.all}</span>
              <span className="hidden @2xl:inline">{m.quarantine.columns.name}</span>
              <span className="hidden @2xl:inline">{m.quarantine.columns.location}</span>
              <span className="hidden text-right @2xl:inline">{m.quarantine.columns.size}</span>
              <span className="hidden @2xl:inline">{m.quarantine.columns.held}</span>
              <span className="hidden @2xl:inline" />
            </div>
          </div>
          <ul className="divide-y divide-line border-b border-line">
            {items.map((item) => (
              <HeldRow
                key={item.operation}
                item={item}
                onDone={(one) => setReported({ one })}
                selected={selected.has(item.operation)}
                onToggle={() => setPicked(toggled(selected, item.operation))}
                locked={working}
              />
            ))}
          </ul>
        </div>
      )}
    </Page>
  );
}
