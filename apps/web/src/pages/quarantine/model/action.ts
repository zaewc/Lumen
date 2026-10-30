import type { HeldItemView, SpaceChangeView } from "@lumen/api-client";
import { useQueryClient } from "@tanstack/react-query";
import { heldTotal } from "@/entities/held";
import { operationOf, QUERY_KEYS, useSessionOperation, useTransport } from "@/shared/api";
import { runBatch } from "../lib/batch";
import type { Done, LastAction } from "../lib/outcome";
import { actionable, type BulkResult, bulkResult } from "../lib/selection";

type Reported = ({ readonly one: LastAction } | { readonly many: BulkResult }) & {
  readonly measurement: {
    readonly changes: readonly SpaceChangeView[] | null;
    readonly expected: string | null;
  } | null;
};
interface Input {
  readonly action: Done["action"];
  readonly items: readonly HeldItemView[];
  readonly mode: "one" | "many";
}
interface Progress {
  readonly done: number;
  readonly total: number;
}
const KEY = ["session-operation", "quarantine"];

/** Confirmed actions keep their exact item snapshot across route changes. */
export function useHeldAction() {
  const transport = useTransport();
  const queries = useQueryClient();
  return useSessionOperation(
    KEY,
    async (
      { action, items, mode }: Input,
      report: (progress: Progress) => void,
    ): Promise<Reported> => {
      const { act } = actionable(action, items);
      const changes: SpaceChangeView[] = [];
      let fullyMeasured = true;
      try {
        const { done, stopped } = await runBatch(
          act,
          async (item): Promise<LastAction> => {
            let result: Done;
            if (action === "restore") {
              result = { action, outcome: (await transport.restore(operationOf(item))).outcome };
            } else {
              const finalized = await transport.finalize(operationOf(item));
              if (finalized.space_change === null) fullyMeasured = false;
              else changes.push(finalized.space_change);
              result = { action, outcome: finalized.outcome };
            }
            return { original: item.original, done: result };
          },
          (count) => report({ done: count, total: act.length }),
        );
        const one = done[0];
        const result =
          mode === "one" && one !== undefined
            ? { one }
            : {
                many: bulkResult(
                  action,
                  done.map((item) => item.done),
                  stopped,
                ),
              };
        // Only finished items count; the one that failed may have changed the
        // disk without a reading, so a stopped run's change stays unknown.
        const estimated = heldTotal(act.slice(0, done.length));
        return {
          ...result,
          measurement:
            action === "finalize"
              ? {
                  changes: fullyMeasured && stopped === null ? changes : null,
                  expected: estimated.exact ? estimated.bytes : null,
                }
              : null,
        };
      } finally {
        // A failure can follow earlier successes; refresh even after a partial batch.
        await queries.invalidateQueries({ queryKey: QUERY_KEYS.held });
        await queries.invalidateQueries({ queryKey: QUERY_KEYS.disk });
      }
    },
  );
}
