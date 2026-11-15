import type { UnreadFoldersView, UnreadReasonView } from "@lumen/api-client";

/**
 * The order reasons are shown in: those a person can act on first (allow
 * Full Disk Access, scan again), then those that are normal, then the rest.
 */
export const REASON_ORDER = [
  "privacy",
  "timed_out",
  "cut_short",
  "permissions",
  "system_protected",
  "out_of_scope",
  "denied",
  "vanished",
  "failed",
  "not_entered",
] as const satisfies readonly UnreadReasonView[];

/** One reason and the folders not read for it, in path order. */
export interface UnreadGroup {
  readonly reason: UnreadReasonView;
  readonly paths: readonly string[];
}

/** The listed folders grouped by reason, in {@link REASON_ORDER}; empty groups left out. */
export function unreadGroups(view: UnreadFoldersView): readonly UnreadGroup[] {
  return REASON_ORDER.flatMap((reason) => {
    const paths = view.folders.filter((f) => f.reason === reason).map((f) => f.path);
    return paths.length === 0 ? [] : [{ reason, paths }];
  });
}

/** How many folders the host did not list, as a decimal string; "0" when all are. */
export function unlistedCount(view: UnreadFoldersView): string {
  const total = /^\d+$/.test(view.total) ? BigInt(view.total) : 0n;
  const rest = total - BigInt(view.folders.length);
  return rest > 0n ? rest.toString() : "0";
}
