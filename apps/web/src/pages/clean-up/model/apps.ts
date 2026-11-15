import type { SoftwareItemView } from "@lumen/api-client";

/** The fewest bytes an app needs to be listed: ADR-0039's floor for large items. */
export const APPS_FLOOR = 100_000_000n;

/** The most apps the section lists; the rest are on the Apps page. */
export const APPS_SHOWN = 10;

/** One app in Explore's Apps section, with what the scan says it takes. */
export interface AppRow {
  readonly item: SoftwareItemView;
  /** Decimal bytes. */
  readonly bytes: string;
  /** Some folder in it could not be read: `bytes` is a lower bound. */
  readonly atLeast: boolean;
}

/** Explore's Apps section (roadmap 16.2, option A). */
export interface AppsSection {
  /** The largest apps of at least [`APPS_FLOOR`], largest first, at most [`APPS_SHOWN`]. */
  readonly rows: readonly AppRow[];
  /** What the listed apps take together, decimal bytes. */
  readonly bytes: string;
  /** Some listed app's size is a lower bound, so `bytes` is one too. */
  readonly atLeast: boolean;
  /** Apps in places this scan did not read: never given a made-up size. */
  readonly outside: number;
}

/**
 * The section for the software list: macOS apps that can be removed (not
 * part of the system) with a size from the scan, largest first. `null` when
 * the scan measured no app at all (it covered no app folder), so the section
 * is left out.
 */
export function appsSection(items: readonly SoftwareItemView[]): AppsSection | null {
  const apps = items.filter((item) => item.origin === "mac_app" && !item.protected);
  const measured: AppRow[] = [];
  let outside = 0;
  for (const item of apps) {
    const size = item.size;
    if (size === null) continue;
    if (size.kind === "not_scanned") {
      outside += 1;
    } else {
      measured.push({ item, bytes: size.bytes, atLeast: size.kind === "at_least" });
    }
  }
  if (measured.length === 0) {
    return null;
  }
  const rows = measured
    .filter((row) => BigInt(row.bytes) >= APPS_FLOOR)
    .sort((a, b) => {
      const [x, y] = [BigInt(a.bytes), BigInt(b.bytes)];
      return x === y ? 0 : x > y ? -1 : 1;
    })
    .slice(0, APPS_SHOWN);
  const bytes = rows.reduce((total, row) => total + BigInt(row.bytes), 0n).toString();
  return { rows, bytes, atLeast: rows.some((row) => row.atLeast), outside };
}
