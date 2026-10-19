import type { Locale } from "./locale";

/**
 * A formatter for counts with the locale's digit grouping ("1,204"). Counts
 * that arrive as decimal strings (ADR-0011) are formatted exactly, as
 * bigints; any other string is shown as it came.
 */
export function countFormat(locale: Locale): (count: number | string) => string {
  const format = new Intl.NumberFormat(locale, { maximumFractionDigits: 0 });
  return (count) => {
    if (typeof count === "number") {
      return format.format(count);
    }
    return /^\d+$/.test(count) ? format.format(BigInt(count)) : count;
  };
}

/** An RFC 3339 timestamp, as the core sends them ("2026-10-16T09:00:00Z"). */
const RFC3339 = /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(\.\d+)?(Z|[+-]\d{2}:\d{2})$/i;

/**
 * An RFC 3339 timestamp as a date and time in the locale's words, in the
 * user's time zone (or `timeZone`, for tests). Anything that is not such a
 * timestamp is shown as it came, never as "Invalid Date".
 */
export function formatDateTime(locale: Locale, iso: string, timeZone?: string): string {
  const time = RFC3339.test(iso) ? Date.parse(iso) : Number.NaN;
  if (!Number.isFinite(time)) {
    return iso;
  }
  return new Intl.DateTimeFormat(locale, {
    dateStyle: "medium",
    timeStyle: "short",
    ...(timeZone === undefined ? {} : { timeZone }),
  }).format(time);
}
