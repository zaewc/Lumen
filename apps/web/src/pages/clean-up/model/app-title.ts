import type { SoftwareItemView } from "@lumen/api-client";

/** The text a row is listed and searched by: its name, or its identifier without one. */
export function softwareTitle(item: SoftwareItemView): string {
  return item.name ?? item.id;
}
