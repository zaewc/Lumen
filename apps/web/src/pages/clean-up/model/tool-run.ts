import { useQueryClient } from "@tanstack/react-query";
import { QUERY_KEYS, useSessionOperation, useTransport } from "@/shared/api";

const KEY = ["session-operation", "tool-command"];

/** One host command at a time, with its output kept when the inspector closes. */
export function useToolRun() {
  const transport = useTransport();
  const queries = useQueryClient();
  return useSessionOperation(KEY, async (entry: string) => {
    if (!transport.runToolCommand) {
      throw new Error("Tool commands are unavailable here");
    }
    try {
      return await transport.runToolCommand(entry);
    } finally {
      await queries.invalidateQueries({ queryKey: QUERY_KEYS.disk });
    }
  });
}
