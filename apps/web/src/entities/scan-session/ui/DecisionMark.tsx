import type { Status } from "../model/summary";

/** A word always accompanies the mark; shape distinguishes decisions without color. */
export function DecisionMark({ status }: { readonly status: Status }) {
  return <span aria-hidden="true" className="decision-mark" data-status={status} />;
}
