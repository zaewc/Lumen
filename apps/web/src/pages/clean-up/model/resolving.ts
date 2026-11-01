import type { DiskView } from "@lumen/api-client";
import { parseBytes } from "@/shared/lib";

/**
 * How much of the disk's used space a scan has read so far, in tenths of a
 * percent (0–1000), or `null` when the disk's usage is not known. It is a
 * share of the disk, never of the scan: a scan's total is unknown until it
 * ends. Hard links and other volumes can read past used space, so it stops
 * at all of it.
 */
export function readPermille(disk: DiskView | null, allocated: string): number | null {
  if (disk === null) {
    return null;
  }
  const total = parseBytes(disk.total_bytes);
  const available = parseBytes(disk.available_bytes);
  const used = available > total ? 0n : total - available;
  if (used === 0n) {
    return null;
  }
  const permille = Number((parseBytes(allocated) * 1000n) / used);
  return Math.min(permille, 1000);
}
