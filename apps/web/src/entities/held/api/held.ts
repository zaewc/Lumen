import type { HeldItemView } from "@lumen/api-client";
import { type UseQueryResult, useQuery } from "@tanstack/react-query";
import { QUERY_KEYS, useTransport } from "@/shared/api";

/** What quarantine holds; refreshed wherever items are moved in or out. */
export function useHeld(): UseQueryResult<readonly HeldItemView[]> {
  const transport = useTransport();
  return useQuery({ queryKey: QUERY_KEYS.held, queryFn: () => transport.held() });
}
