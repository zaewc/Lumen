import type { AiSettingsView } from "@lumen/api-client";
import { type UseQueryResult, useQuery } from "@tanstack/react-query";
import { QUERY_KEYS, useTransport } from "@/shared/api";

/** Whether AI review is on, and which key is stored (ADR-0037). */
export function useAiSettings(): UseQueryResult<AiSettingsView> {
  const transport = useTransport();
  return useQuery({
    queryKey: QUERY_KEYS.aiSettings,
    queryFn: () => transport.aiSettings(),
    retry: false,
    refetchOnWindowFocus: false,
  });
}
