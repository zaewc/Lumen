import type { SpaceChangeView } from "@lumen/api-client";
import { parseBytes } from "./bytes";

/** Compare the first and last readings of each volume, never sum overlapping intervals. */
export function summarizeSpaceChanges(changes: readonly SpaceChangeView[] | null) {
  if (changes === null || changes.length === 0) return null;
  const volumes = new Map<string, SpaceChangeView>();
  try {
    for (const change of changes) {
      parseBytes(change.before_available_bytes);
      parseBytes(change.after_available_bytes);
      const first = volumes.get(change.volume);
      volumes.set(change.volume, {
        ...change,
        before_available_bytes: first?.before_available_bytes ?? change.before_available_bytes,
      });
    }
    const readings = [...volumes.values()];
    const delta = readings.reduce(
      (total, change) =>
        total +
        parseBytes(change.after_available_bytes) -
        parseBytes(change.before_available_bytes),
      0n,
    );
    return { readings, delta };
  } catch {
    return null;
  }
}
