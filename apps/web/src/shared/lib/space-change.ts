import type { SpaceChangeView } from "@lumen/api-client";
import { parseBytes } from "./bytes";

/** Compare each volume separately: different volumes can share the same free-space pool. */
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
    const only = readings.length === 1 ? readings[0] : undefined;
    const delta = only
      ? parseBytes(only.after_available_bytes) - parseBytes(only.before_available_bytes)
      : null;
    return { readings, delta };
  } catch {
    return null;
  }
}
