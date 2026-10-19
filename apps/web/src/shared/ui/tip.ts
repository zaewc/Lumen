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
