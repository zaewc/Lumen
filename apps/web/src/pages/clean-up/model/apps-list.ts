import type { SoftwareItemView, SoftwareSourceView } from "@lumen/api-client";

/** What the list shows: a search, and whether dependencies and hidden items are included. */
export interface Filter {
  readonly query: string;
  readonly all: boolean;
}

export { softwareTitle as title } from "./app-title";

/**
 * The rows the filter keeps. Without `all`, packages installed only as
 * another package's dependency, and entries the platform's own list hides,
 * are left out; a row whose record does not say why it was installed stays.
 */
export function visible(
  items: readonly SoftwareItemView[],
  filter: Filter,
): readonly SoftwareItemView[] {
  const words = filter.query.trim().toLocaleLowerCase().split(/\s+/u).filter(Boolean);
  return items.filter((item) => {
    if (!filter.all && (item.explicit === false || item.hidden)) return false;
    const text = [item.name, item.id, item.vendor, item.version]
      .filter((part): part is string => part !== null)
      .join(" ")
      .toLocaleLowerCase();
    return words.every((word) => text.includes(word));
  });
}

/** How many rows the filter leaves out of `items` because of `all` alone. */
export function tucked(items: readonly SoftwareItemView[]): number {
  return items.filter((item) => item.explicit === false || item.hidden).length;
}

/**
 * Whether rows leave room for an app icon: only when the list has macOS apps,
 * the only rows the host draws icons for so far (roadmap 16.4). Every row
 * then gets the same room, so names line up; elsewhere there is none.
 */
export function iconsShown(items: readonly SoftwareItemView[]): boolean {
  return items.some((item) => item.origin === "mac_app");
}

/** The inventories read only in part: some software may be missing from the list. */
export function partial(sources: readonly SoftwareSourceView[]): readonly string[] {
  return sources.filter((source) => source.state === "partial").map((source) => source.source);
}

/** How the list is ordered: by name (as the host lists it) or by size. */
export type Order = "name" | "size";

/** The bytes the latest scan gives an app, a lower bound included; `null` when it gives none. */
export function scanBytes(item: SoftwareItemView): bigint | null {
  const size = item.size;
  return size === null || size.kind === "not_scanned" ? null : BigInt(size.bytes);
}

/**
 * `items` in `order`. By size: the apps the scan measured, largest first, then
 * everything else in the host's order (unknown sizes are never sorted as zero).
 */
export function ordered(
  items: readonly SoftwareItemView[],
  order: Order,
): readonly SoftwareItemView[] {
  if (order === "name") {
    return items;
  }
  const sized = items.filter((item) => scanBytes(item) !== null);
  const rest = items.filter((item) => scanBytes(item) === null);
  const bySize = sized.toSorted((a, b) => {
    const [x, y] = [scanBytes(a) ?? 0n, scanBytes(b) ?? 0n];
    return x === y ? 0 : x > y ? -1 : 1;
  });
  return [...bySize, ...rest];
}
