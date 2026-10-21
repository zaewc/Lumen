import type { DiskView } from "@lumen/api-client";
import { parseBytes } from "@/shared/lib";

/** How full a disk is, worked out exactly from the core's decimal byte counts. */
export interface DiskUsage {
  /** Decimal byte counts. */
  readonly total: string;
  readonly used: string;
  readonly available: string;
  /** The used share, in tenths of a percent (0–1000), rounded down. */
  readonly usedPermille: number;
  /** Less than a tenth of the disk is available. */
  readonly low: boolean;
}

/**
 * The usage of a disk. Used space is what is not available; an available
 * figure larger than the disk (a stale or odd report) counts as all of it.
 * Throws on a count that is not a decimal string.
 */
export function diskUsage(view: DiskView): DiskUsage {
  const total = parseBytes(view.total_bytes);
  const reported = parseBytes(view.available_bytes);
  const available = reported > total ? total : reported;
  const used = total - available;
  return {
    total: total.toString(),
    used: used.toString(),
    available: available.toString(),
    usedPermille: total === 0n ? 0 : Number((used * 1000n) / total),
    low: total > 0n && available * 10n < total,
  };
}

/** A permille as a percentage with one decimal, for people: "96.9". */
export function percentText(permille: number): string {
  return `${Math.trunc(permille / 10)}.${permille % 10}`;
}
