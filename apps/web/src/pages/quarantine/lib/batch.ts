/** Why a run over several items stopped early, after some were already done. */
export interface Stopped {
  /** What the transport rejected with, for "Details". */
  readonly error: unknown;
  /** Items after the one that failed, which were never tried. */
  readonly untried: number;
}

/** What each finished item did, in order, and what stopped the run, if anything. */
export interface Batch<Result> {
  readonly done: Result[];
  readonly stopped: Stopped | null;
}

/**
 * Runs `step` on each item in turn, reporting how many are done before each.
 * A failure after earlier items finished does not lose them: those items are
 * already restored or removed for good, so the run ends there and says so.
 * A failure before anything finished propagates as it is.
 */
export async function runBatch<Item, Result>(
  items: readonly Item[],
  step: (item: Item) => Promise<Result>,
  report: (done: number) => void,
): Promise<Batch<Result>> {
  const done: Result[] = [];
  for (const [index, item] of items.entries()) {
    report(index);
    try {
      done.push(await step(item));
    } catch (error) {
      if (done.length === 0) {
        throw error;
      }
      return { done, stopped: { error, untried: items.length - index - 1 } };
    }
  }
  report(items.length);
  return { done, stopped: null };
}
