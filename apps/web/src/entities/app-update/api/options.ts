import { queryOptions } from "@tanstack/react-query";
import { type LumenTransport, QUERY_KEYS } from "@/shared/api";

/** Check at startup, every six hours, and on returning with stale results. */
export function updateOptions(check: LumenTransport["checkUpdate"]) {
  const recheck = 6 * 60 * 60 * 1000;
  return queryOptions({
    queryKey: QUERY_KEYS.update,
    queryFn: () => (check ? check() : Promise.reject(new Error("no updater"))),
    enabled: check !== undefined,
    staleTime: recheck,
    refetchInterval: recheck,
    refetchOnWindowFocus: true,
    retry: false,
  });
}
