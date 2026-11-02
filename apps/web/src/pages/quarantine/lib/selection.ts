import type { HeldItemView } from "@lumen/api-client";
import type { Messages } from "@/shared/i18n";
import type { Stopped } from "./batch";
import { type Done, doneWell, type LastAction } from "./outcome";

/** The selected items, by operation. */
export type Selection = ReadonlySet<string>;

/** Whether every, some or none of `items` is selected (for "Select all"). */
export function selectionState(
  items: readonly HeldItemView[],
  selected: Selection,
): "all" | "some" | "none" {
  const count = items.filter((item) => selected.has(item.operation)).length;
  if (count === 0) {
    return "none";
  }
  return count === items.length ? "all" : "some";
}

/** `selected` with `operation` added or taken out. */
export function toggled(selected: Selection, operation: string): Selection {
  const next = new Set(selected);
  if (!next.delete(operation)) {
    next.add(operation);
  }
  return next;
}

/** The selection kept to items still held: restored or removed ones drop out. */
export function stillHeld(items: readonly HeldItemView[], selected: Selection): Selection {
  return new Set(
    items.filter((item) => selected.has(item.operation)).map((item) => item.operation),
  );
}

/** The selected items, in list order. */
export function chosen(items: readonly HeldItemView[], selected: Selection): HeldItemView[] {
  return items.filter((item) => selected.has(item.operation));
}

/**
 * What one action on several items can act on: every item can be restored,
 * but only retained items can be removed for good (an item held after a
 * failed move can only be restored).
 */
export function actionable(
  action: Done["action"],
  items: readonly HeldItemView[],
): { readonly act: HeldItemView[]; readonly skipped: number } {
  const act = action === "restore" ? [...items] : items.filter((i) => i.state === "retained");
  return { act, skipped: items.length - act.length };
}

/** How an action on several items ended. */
export interface BulkResult {
  readonly action: Done["action"];
  /** Items asked for (including any an error kept Lumen from finishing or trying). */
  readonly total: number;
  /** Items that came out as asked (restored and verified, or removed). */
  readonly well: number;
  /** Items put back where they were that do not exactly match their record. */
  readonly unverified: number;
  /** Finished items that did not come out as asked, each with what happened. */
  readonly misses: readonly LastAction[];
  /** The error that stopped the run early, after `well` items were already done. */
  readonly stopped: Stopped | null;
}

/** The result of `action`, from each finished item's outcome and what stopped the run. */
export function bulkResult(
  action: Done["action"],
  done: readonly LastAction[],
  stopped: Stopped | null = null,
): BulkResult {
  const unfinished = stopped === null ? 0 : stopped.untried + 1;
  const outcomes = done.map((item) => item.done);
  const unverified = outcomes.filter(
    (d) => d.action === "restore" && d.outcome.kind === "restored" && !d.outcome.verified,
  ).length;
  return {
    action,
    total: done.length + unfinished,
    well: outcomes.filter(doneWell).length,
    unverified,
    misses: done.filter((item) => !doneWell(item.done)),
    stopped,
  };
}

/** The result in words. */
export function bulkText(m: Messages, result: BulkResult): string {
  const text = m.quarantine.selection;
  const { well, unverified, total, stopped } = result;
  switch (result.action) {
    case "restore": {
      // An unverified item is no longer in Quarantine: it counts as restored.
      const back = well + unverified;
      const said =
        stopped === null
          ? text.restored(back, total)
          : text.restoredStopped(back, total, stopped.untried);
      return unverified === 0 ? said : `${said} ${text.unverified(unverified)}`;
    }
    case "finalize":
      return stopped === null
        ? text.removed(well, total)
        : text.removedStopped(well, total, stopped.untried);
  }
}
