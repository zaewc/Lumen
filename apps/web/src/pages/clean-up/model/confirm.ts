import type { CandidateView } from "@lumen/api-client";
import { baseName, parentPath } from "@/shared/lib";

/** One item a plan would move, as the confirmation lists it. */
export interface PlanRow {
  readonly id: string;
  readonly name: string;
  readonly folder: string;
  readonly path: string;
  /** Decimal byte count. */
  readonly bytes: string;
}

/** A plan's items, largest first (ties keep plan order), so the list reads like the map. */
export function planRows(items: readonly CandidateView[]): PlanRow[] {
  return items
    .map((item) => ({
      id: item.id,
      name: baseName(item.path),
      folder: parentPath(item.path),
      path: item.path,
      bytes: item.reclaimable_bytes,
    }))
    .sort((a, b) => {
      const [x, y] = [BigInt(a.bytes), BigInt(b.bytes)];
      return x === y ? 0 : x > y ? -1 : 1;
    });
}
