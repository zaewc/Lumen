/** What a modal dialog element must do to match the state it should be in. */
export type DialogChange = "show" | "close" | "none";

/**
 * Keeps a `<dialog>` in step with its React state: show it when it should be
 * open and is not, close it when it should be closed and is open.
 */
export function dialogChange(wanted: boolean, shown: boolean): DialogChange {
  if (wanted === shown) {
    return "none";
  }
  return wanted ? "show" : "close";
}
