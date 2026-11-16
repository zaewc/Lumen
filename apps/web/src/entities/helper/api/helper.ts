import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect } from "react";
import { useTransport } from "@/shared/api";
import type { HelperAction } from "../model/helper";

const HELPER = ["helper"] as const;

/**
 * The administrator helper's state (ADR-0063), and changing it. Absent off
 * macOS (`read` is undefined). The state is asked again when the window
 * regains focus, since allowing the helper happens in System Settings: on
 * the window's `focus` event too, which TanStack Query's own handler (only
 * `visibilitychange`) misses when the webview stays visible.
 */
export function useHelper() {
  const transport = useTransport();
  const queries = useQueryClient();
  const read = transport.helperState;
  const state = useQuery({
    queryKey: HELPER,
    queryFn: () => (read === undefined ? Promise.reject(new Error("unsupported")) : read()),
    enabled: read !== undefined,
    refetchOnWindowFocus: true,
  });
  const { refetch } = state;
  useEffect(() => {
    if (read === undefined) return;
    const again = () => void refetch();
    window.addEventListener("focus", again);
    return () => window.removeEventListener("focus", again);
  }, [read, refetch]);
  const change = useMutation({
    mutationFn: async (action: HelperAction) => {
      switch (action) {
        case "enable":
          return transport.helperEnable?.();
        case "disable":
          return transport.helperDisable?.();
        case "open_login_items":
          await transport.helperOpenLoginItems?.();
          return undefined;
      }
    },
    onSettled: () => queries.invalidateQueries({ queryKey: HELPER }),
  });
  return { supported: read !== undefined, state, change };
}
