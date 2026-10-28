import type { UpdateView } from "@lumen/api-client";
import { type UseQueryResult, useQuery } from "@tanstack/react-query";
import { QUERY_KEYS, useTransport } from "@/shared/api";

/** How often the app looks for a new version while it stays open. */
const RECHECK_MS = 6 * 60 * 60 * 1000;

/**
 * Whether a newer version is published (ADR-0048), checked when the app
 * starts and every few hours; disabled where the host cannot update itself.
 * A failed check is quiet: it is shown only in Settings.
 */
export function useUpdate(): UseQueryResult<UpdateView> {
  const transport = useTransport();
  const check = transport.checkUpdate;
  return useQuery({
    queryKey: QUERY_KEYS.update,
    queryFn: () => (check ? check() : Promise.reject(new Error("no updater"))),
    enabled: check !== undefined,
    staleTime: RECHECK_MS,
    refetchInterval: RECHECK_MS,
    refetchOnWindowFocus: false,
    retry: false,
  });
}
