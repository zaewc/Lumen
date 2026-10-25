/** How far Page Up and Page Down move through a list. */
export const PAGE_STEP = 10;

/**
 * Where a key moves the selection in a list of `count` rows, as Finder's
 * list does: ↑ and ↓ by one, Page Up and Page Down by a page, Home and End
 * to the ends, stopping at the edges. `null` for any other key, or an empty
 * list, so the key keeps its usual meaning.
 */
export function moveIndex(current: number, count: number, key: string): number | null {
  if (count === 0) {
    return null;
  }
  const clamp = (index: number) => Math.min(Math.max(index, 0), count - 1);
  switch (key) {
    case "ArrowDown":
      return clamp(current + 1);
    case "ArrowUp":
      return clamp(current - 1);
    case "PageDown":
      return clamp(current + PAGE_STEP);
    case "PageUp":
      return clamp(current - PAGE_STEP);
    case "Home":
      return 0;
    case "End":
      return count - 1;
    default:
      return null;
  }
}
