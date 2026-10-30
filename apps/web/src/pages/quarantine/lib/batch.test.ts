import { describe, expect, it } from "vitest";
import { runBatch } from "./batch";

describe("runBatch", () => {
  const failAt = (bad: number) => async (n: number) => {
    if (n === bad) {
      throw { message: "another command is still running" };
    }
    return n * 10;
  };

  it("runs every item in order and reports progress", async () => {
    const seen: number[] = [];
    const batch = await runBatch([1, 2, 3], failAt(-1), (done) => seen.push(done));
    expect(batch).toEqual({ done: [10, 20, 30], stopped: null });
    expect(seen).toEqual([0, 1, 2, 3]);
  });

  it("keeps items already done when a later one fails, and counts the untried", async () => {
    const items = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
    const tried: number[] = [];
    const batch = await runBatch(
      items,
      async (n) => {
        tried.push(n);
        return failAt(4)(n);
      },
      () => undefined,
    );
    expect(batch.done).toEqual([10, 20, 30]);
    expect(batch.stopped).toEqual({
      error: { message: "another command is still running" },
      untried: 6,
    });
    // Nothing after the failure is attempted.
    expect(tried).toEqual([1, 2, 3, 4]);
  });

  it("stops with nothing untried when the last item fails", async () => {
    const batch = await runBatch([1, 2], failAt(2), () => undefined);
    expect(batch.done).toEqual([10]);
    expect(batch.stopped?.untried).toBe(0);
  });

  it("propagates a failure before anything was done", async () => {
    await expect(runBatch([1, 2, 3], failAt(1), () => undefined)).rejects.toEqual({
      message: "another command is still running",
    });
  });
});
