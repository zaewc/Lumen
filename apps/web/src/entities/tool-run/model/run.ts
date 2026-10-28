import { useSessionOperation, useTransport } from "@/shared/api";

const KEY = ["session-operation", "tool-command"];

/** One host command at a time, with its output kept when the inspector closes. */
export function useToolRun() {
  const transport = useTransport();
  return useSessionOperation(KEY, async (entry: string) => {
    if (!transport.runToolCommand) {
      throw new Error("Tool commands are unavailable here");
    }
    return transport.runToolCommand(entry);
  });
}
