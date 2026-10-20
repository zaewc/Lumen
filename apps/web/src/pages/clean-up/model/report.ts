import type { ItemReportView, QuarantineReportView } from "@lumen/api-client";

export interface ReportSummary {
  /** How many items are now in quarantine. */
  readonly moved: number;
  /** The items that were skipped or failed, to explain. */
  readonly notMoved: readonly ItemReportView[];
}

/** What a clean-up did, counted. */
export function reportSummary(report: QuarantineReportView): ReportSummary {
  const moved = report.items.filter((item) => item.result.kind === "quarantined");
  const notMoved = report.items.filter((item) => item.result.kind !== "quarantined");
  return { moved: moved.length, notMoved };
}
