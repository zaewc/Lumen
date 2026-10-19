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
