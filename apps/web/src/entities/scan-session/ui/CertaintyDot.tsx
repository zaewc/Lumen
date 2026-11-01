import type { Certainty } from "../model/certainty";

/**
 * How certain a fact is, drawn the same way everywhere: solid when resolved,
 * half-filled when partial, open when unknown. Words beside it carry the
 * meaning; the dot never stands alone.
 */
export function CertaintyDot({ certainty }: { readonly certainty: Certainty }) {
  return <span aria-hidden="true" className="certainty-dot" data-certainty={certainty} />;
}
