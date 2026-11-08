import { queryOptions, useQueries } from "@tanstack/react-query";
import { idleOperation, type OperationState } from "./operation";

/**
 * The session operations that move or remove items. While one runs, the
 * desktop host holds quitting and updates (UX-3).
 */
export const MOVING_OPERATIONS = {
  app: ["session-operation", "app-quarantine"],
  cleanUp: ["session-operation", "clean-up"],
  quarantine: ["session-operation", "quarantine"],
} as const;

/** Whether a clean-up or a Quarantine action is running in this session. */
export function useMoving(): boolean {
  return useQueries({
    queries: Object.values(MOVING_OPERATIONS).map((key) =>
      queryOptions<OperationState<unknown, unknown, unknown>>({
        queryKey: key,
        initialData: idleOperation<unknown, unknown, unknown>,
        enabled: false,
        staleTime: Number.POSITIVE_INFINITY,
        gcTime: Number.POSITIVE_INFINITY,
      }),
    ),
    combine: (results) => results.some((result) => result.data.status === "pending"),
  });
}

/** Calls `held` each time the host holds a quit request; returns the cleanup. */
export function followQuitHeld(target: EventTarget, held: () => void): () => void {
  target.addEventListener("lumen:quit-held", held);
  return () => target.removeEventListener("lumen:quit-held", held);
}
