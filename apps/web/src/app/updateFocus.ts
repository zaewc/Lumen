import type { QueryClient } from "@tanstack/react-query";
import { UPDATE_RECHECK_MS } from "@/entities/app-update";
import { QUERY_KEYS } from "@/shared/api";

/** Refresh only stale, active update queries on actual window focus. */
export function followUpdateFocus(queries: QueryClient, target: EventTarget): () => void {
  const refresh = () => {
    void queries.refetchQueries(
      {
        queryKey: QUERY_KEYS.update,
        exact: true,
        type: "active",
        predicate: (query) => query.isStaleByTime(UPDATE_RECHECK_MS),
      },
      { cancelRefetch: false },
    );
  };
  target.addEventListener("focus", refresh);
  target.addEventListener("lumen:window-focus", refresh);
  return () => {
    target.removeEventListener("focus", refresh);
    target.removeEventListener("lumen:window-focus", refresh);
  };
}
