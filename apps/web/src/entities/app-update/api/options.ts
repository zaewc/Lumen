import { queryOptions } from "@tanstack/react-query";
import { type LumenTransport, QUERY_KEYS } from "@/shared/api";

export const UPDATE_RECHECK_MS = 6 * 60 * 60 * 1000;

/** Check at startup, every six hours, and on returning with stale results. */
export function updateOptions(check: LumenTransport["checkUpdate"]) {
  return queryOptions({
    queryKey: QUERY_KEYS.update,
    queryFn: () => (check ? check() : Promise.reject(new Error("no updater"))),
    enabled: check !== undefined,
    staleTime: UPDATE_RECHECK_MS,
    refetchInterval: UPDATE_RECHECK_MS,
    refetchOnWindowFocus: true,
    retry: false,
  });
}
