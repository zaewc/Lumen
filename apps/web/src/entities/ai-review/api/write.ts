import { useMutationState } from "@tanstack/react-query";

export const AI_WRITE_KEY = ["ai-settings-write"];

/** Share only status/error. A typed API key never enters session-operation data. */
export function useAiWriteState() {
  return useMutationState({
    filters: { mutationKey: AI_WRITE_KEY },
    select: (mutation) => ({ status: mutation.state.status, error: mutation.state.error }),
  }).at(-1);
}
