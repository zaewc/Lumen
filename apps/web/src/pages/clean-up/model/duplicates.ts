import type { DuplicatesView } from "@lumen/api-client";

/** At most this many groups are listed, largest first; the rest are counted. */
export const SHOWN_GROUPS = 100;

/**
 * What a comparison did not cover, or found but does not count as copies;
 * each only when it happened, so an incomplete comparison never reads as
 * a complete one.
 */
export type CompareNote =
  | { readonly kind: "hard_links"; readonly count: number }
  | { readonly kind: "skipped"; readonly count: number }
  | { readonly kind: "protected"; readonly count: number }
  | { readonly kind: "incomplete" };

export function compareNotes(report: DuplicatesView): readonly CompareNote[] {
  const notes: CompareNote[] = [];
  const counted = [
    ["hard_links", report.hard_links.length],
    ["skipped", Number(report.skipped)],
    ["protected", Number(report.protected)],
  ] as const;
  for (const [kind, count] of counted) {
    if (count > 0) notes.push({ kind, count });
  }
  if (report.incomplete) notes.push({ kind: "incomplete" });
  return notes;
}
