// Byte counts arrive as decimal strings (ADR-0011), so no size is ever
// rounded by JavaScript numbers before it is shown.

const UNITS = ["B", "kB", "MB", "GB", "TB", "PB", "EB"] as const;

/** A decimal byte count as a bigint. Throws on a non-decimal string. */
export function parseBytes(decimal: string): bigint {
  if (!/^\d+$/.test(decimal)) {
    throw new Error(`not a byte count: ${decimal}`);
  }
  return BigInt(decimal);
}

/** The exact sum of decimal byte counts, as a decimal string. */
export function sumBytes(decimals: Iterable<string>): string {
  let total = 0n;
  for (const decimal of decimals) {
    total += parseBytes(decimal);
  }
  return total.toString();
}

/**
 * A byte count in decimal units, as Finder and Explorer show sizes,
 * truncated to one decimal place: "4.2 GB". Throws on a non-decimal string.
 */
export function formatBytes(decimal: string): string {
  let value = parseBytes(decimal);
  let unit = 0;
  let remainder = 0n;
  while (value >= 1000n && unit + 1 < UNITS.length) {
    remainder = value % 1000n;
    value /= 1000n;
    unit += 1;
  }
  const name = UNITS[unit] ?? "B";
  return unit === 0 ? `${value} ${name}` : `${value}.${remainder / 100n} ${name}`;
}
