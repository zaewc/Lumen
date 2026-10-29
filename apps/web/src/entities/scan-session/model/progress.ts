import type { AssessProgressView } from "@lumen/api-client";

/** Whole percent of this stage's real units; unknown totals stay indeterminate. */
export function stagePercent(progress: AssessProgressView | null): number | null {
  if (!progress || progress.total === null) return null;
  const decimal = /^(0|[1-9]\d{0,19})$/;
  if (!decimal.test(progress.completed) || !decimal.test(progress.total)) return null;
  const completed = BigInt(progress.completed);
  const total = BigInt(progress.total);
  if (total === 0n || completed > total) return null;
  return Number((completed * 100n) / total);
}
