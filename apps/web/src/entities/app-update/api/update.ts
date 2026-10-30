import type { UpdateView } from "@lumen/api-client";
import { type UseQueryResult, useQuery } from "@tanstack/react-query";
import { useTransport } from "@/shared/api";
import { updateOptions } from "./options";

/**
 * Whether a newer version is published (ADR-0048), checked when the app
 * starts and every few hours; disabled where the host cannot update itself.
 * A failed check is quiet: it is shown only in Settings.
 */
export function useUpdate(): UseQueryResult<UpdateView> {
  const transport = useTransport();
  return useQuery(updateOptions(transport.checkUpdate));
}
