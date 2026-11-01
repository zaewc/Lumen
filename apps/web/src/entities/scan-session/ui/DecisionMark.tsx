import type { Status } from "../model/summary";

/**
 * What a mark can show: a decision, or what became of a quarantined item
 * (held and restorable, or interrupted and needing attention).
 */
export type Mark = Status | "held" | "attention";

/** A word always accompanies the mark; shape distinguishes decisions without color. */
export function DecisionMark({ status }: { readonly status: Mark }) {
  return <span aria-hidden="true" className="decision-mark" data-status={status} />;
}
