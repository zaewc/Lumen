/** What can happen to an info tip. */
export type TipEvent = "toggle" | "escape" | "outside";

/** A tip's next state, and whether focus goes back to its button. */
export interface TipChange {
  readonly open: boolean;
  readonly focusButton: boolean;
}

/**
 * The disclosure logic of an info tip: its button toggles it; Escape and a
 * click outside close it, and Escape on an open tip returns focus to the
 * button so keyboard users are not left stranded.
 */
export function nextTip(open: boolean, event: TipEvent): TipChange {
  switch (event) {
    case "toggle":
      return { open: !open, focusButton: false };
    case "escape":
      return { open: false, focusButton: open };
    case "outside":
      return { open: false, focusButton: false };
  }
}

/** The space (px, 1rem) a tip's panel keeps from each edge of the viewport. */
export const TIP_GUTTER = 16;

/**
 * Where a tip's panel goes, as a shift (px) from its button's left edge: it
 * opens under the button and slides left only as far as it must to end
 * `gutter` px inside the viewport. A panel wider than the room left keeps
 * its left edge in view, so its start is never cut off.
 */
export function panelShift(
  buttonLeft: number,
  panelWidth: number,
  viewportWidth: number,
  gutter = TIP_GUTTER,
): number {
  const left = Math.max(gutter, Math.min(buttonLeft, viewportWidth - gutter - panelWidth));
  return left - buttonLeft;
}
