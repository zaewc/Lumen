import type { SoftwareSourceView } from "@lumen/api-client";

export { softwareTitle as title } from "./app-title";

/** The inventories read only in part: some software may be missing from the list. */
export function partial(sources: readonly SoftwareSourceView[]): readonly string[] {
  return sources.filter((source) => source.state === "partial").map((source) => source.source);
}
