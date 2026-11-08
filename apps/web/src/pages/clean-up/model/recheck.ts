import type { ReassessView } from "@lumen/api-client";
import { type Status, statusOf } from "@/entities/scan-session";

/** How checking one item again ended, for the sentence that says so. */
export type Rechecked =
  | { readonly kind: "settled" }
  | { readonly kind: "changed" }
  | { readonly kind: "still"; readonly status: Status }
  | { readonly kind: "now"; readonly before: Status; readonly after: Status }
  | { readonly kind: "missing" };

/** How item `id`, which was `before`, ended in `view`. */
export function recheckedOf(view: ReassessView, id: string, before: Status): Rechecked {
  const item = view.items.find((one) => one.id === id);
  if (item === undefined) return { kind: "missing" };
  switch (item.outcome.kind) {
    case "settled":
      return { kind: "settled" };
    case "changed":
      return { kind: "changed" };
    case "decided": {
      const after = statusOf(item.outcome.candidate);
      return after === before ? { kind: "still", status: after } : { kind: "now", before, after };
    }
    default: {
      const unknown: never = item.outcome;
      return unknown;
    }
  }
}
