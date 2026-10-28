import { useQuery, useQueryClient } from "@tanstack/react-query";
import { idleOperation, type OperationState, sessionOperation } from "./operation";

/** Subscribe to confirmed work without tying its lifetime to this component. */
export function useSessionOperation<Input, Output, Progress = never>(
  key: readonly string[],
  execute: (input: Input, report: (progress: Progress) => void) => Promise<Output>,
) {
  const queries = useQueryClient();
  const state = useQuery<OperationState<Input, Output, Progress>>({
    queryKey: key,
    initialData: idleOperation<Input, Output, Progress>,
    enabled: false,
    staleTime: Number.POSITIVE_INFINITY,
    gcTime: Number.POSITIVE_INFINITY,
  });
  const operation = sessionOperation(queries, key, execute);
  return { ...state.data, run: operation.run, reset: operation.reset };
}
