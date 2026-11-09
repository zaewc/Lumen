import type { SoftwareItemView, SoftwareSourceView } from "@lumen/api-client";

/** What the list shows: a search, and whether dependencies and hidden items are included. */
export interface Filter {
  readonly query: string;
  readonly all: boolean;
}

/** The text a row is listed and searched by: its name, or its identifier without one. */
export function title(item: SoftwareItemView): string {
  return item.name ?? item.id;
}

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
