import type { HeldItemView } from "@lumen/api-client";
import type { Messages } from "@/shared/i18n";
import { type Done, doneWell } from "./outcome";

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
  /** Items acted on. */
  readonly total: number;
  /** Items that came out as asked (restored and verified, or removed). */
  readonly well: number;
}

/** The result of `action`, from each item's outcome. */
export function bulkResult(action: Done["action"], done: readonly Done[]): BulkResult {
  return { action, total: done.length, well: done.filter(doneWell).length };
}

/** The result in words. */
export function bulkText(m: Messages, result: BulkResult): string {
  const text = m.quarantine.selection;
  return result.action === "restore"
    ? text.restored(result.well, result.total)
    : text.removed(result.well, result.total);
}
