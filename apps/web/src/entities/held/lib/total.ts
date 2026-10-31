import type { HeldItemView } from "@lumen/api-client";
import { sumBytes } from "@/shared/lib";

/** What the held items would free together, and whether every size is known. */
export interface HeldTotal {
  /** Decimal byte count of the items whose size is known. */
  readonly bytes: string;
  /** False when some item was quarantined before sizes were recorded. */
  readonly exact: boolean;
}

/**
 * The items Quarantine holds: without those an interrupted clean-up left
 * where Lumen can't tell (`needs_attention`), which may not be in it.
 */
export function inQuarantine(items: readonly HeldItemView[]): HeldItemView[] {
  return items.filter((item) => item.state !== "needs_attention");
}

/** The space `items` would free if all were removed for good. */
export function heldTotal(items: readonly HeldItemView[]): HeldTotal {
  const known = items.flatMap((item) => (item.reclaim_bytes === null ? [] : [item.reclaim_bytes]));
  return { bytes: sumBytes(known), exact: known.length === items.length };
}
