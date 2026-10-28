import type { HeldItemView } from "@lumen/api-client";
import { useQueryClient } from "@tanstack/react-query";
import { operationOf, QUERY_KEYS, useSessionOperation, useTransport } from "@/shared/api";
import type { Done, LastAction } from "../lib/outcome";
import { actionable, type BulkResult, bulkResult } from "../lib/selection";

type Reported = { readonly one: LastAction } | { readonly many: BulkResult };
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
      const done: LastAction[] = [];
      try {
        for (const [index, item] of act.entries()) {
          report({ done: index, total: act.length });
          const result: Done =
            action === "restore"
              ? { action, outcome: (await transport.restore(operationOf(item))).outcome }
              : { action, outcome: (await transport.finalize(operationOf(item))).outcome };
          done.push({ original: item.original, done: result });
        }
        report({ done: act.length, total: act.length });
        const one = done[0];
        return mode === "one" && one !== undefined
          ? { one }
          : {
              many: bulkResult(
                action,
                done.map((item) => item.done),
              ),
            };
      } finally {
        // A failure can follow earlier successes; refresh even after a partial batch.
        await queries.invalidateQueries({ queryKey: QUERY_KEYS.held });
        await queries.invalidateQueries({ queryKey: QUERY_KEYS.disk });
      }
    },
  );
}
