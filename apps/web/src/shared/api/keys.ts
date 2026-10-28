/**
 * The query keys more than one part of the app reads or refreshes, so a
 * change in one place (a clean-up, a restore) updates every view of it.
 */
export const QUERY_KEYS = {
  /** What quarantine holds. */
  held: ["held"],
  /** Whether AI review is on. */
  aiSettings: ["ai-settings"],
  /** How full the disk is. */
  disk: ["disk"],
  /** Whether a newer version is published. */
  update: ["update"],
  /** The assessment of a scan (followed by the scan's ID). */
  assessment: ["assessment"],
} as const;
