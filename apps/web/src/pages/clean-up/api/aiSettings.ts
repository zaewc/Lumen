import type { AiSettingsView } from "@lumen/api-client";
import { type UseQueryResult, useQuery } from "@tanstack/react-query";
import { useTransport } from "@/shared/api";

/** The query key of the AI review setting; changes write their answer here. */
export const AI_SETTINGS = ["ai-settings"] as const;

/** Whether AI review is on, and which key is stored (ADR-0037). */
export function useAiSettings(): UseQueryResult<AiSettingsView> {
  const transport = useTransport();
  return useQuery({
    queryKey: AI_SETTINGS,
    queryFn: () => transport.aiSettings(),
    retry: false,
    refetchOnWindowFocus: false,
  });
}
