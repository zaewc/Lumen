import type { QueryClient } from "@tanstack/react-query";

/** Confirmed work belongs to the application session, not its current view. */
export type OperationState<Input, Output, Progress = never> = {
  readonly status: "idle" | "pending" | "success" | "error";
  readonly variables: Input | null;
  readonly data: Output | null;
  readonly error: unknown;
  readonly progress: Progress | null;
};

export function idleOperation<Input, Output, Progress>(): OperationState<Input, Output, Progress> {
  return { status: "idle", variables: null, data: null, error: null, progress: null };
}

/**
 * Owns one operation in the session cache. A view can disappear and subscribe
 * again without losing its progress, result or error. A second run cannot
 * overlap it, and resetting a view never cancels work already confirmed.
 * Keys are internal, unique per operation type; no state is persisted to disk.
 */
export function sessionOperation<Input, Output, Progress = never>(
  queries: QueryClient,
  key: readonly string[],
  execute: (input: Input, report: (progress: Progress) => void) => Promise<Output>,
) {
  const read = () =>
    queries.getQueryData<OperationState<Input, Output, Progress>>(key) ??
    idleOperation<Input, Output, Progress>();
  const write = (state: OperationState<Input, Output, Progress>) => {
    queries.setQueryDefaults(key, { gcTime: Number.POSITIVE_INFINITY });
    const cached = queries.getQueryCache().find({ queryKey: key, exact: true });
    cached?.setOptions({ ...cached.options, gcTime: Number.POSITIVE_INFINITY });
    queries.setQueryData(key, state);
  };
  return {
    read,
    reset: () => {
      if (read().status !== "pending") {
        write(idleOperation());
      }
    },
    run: async (variables: Input): Promise<void> => {
      if (read().status === "pending") {
        return;
      }
      write({ status: "pending", variables, data: null, error: null, progress: null });
      try {
        const data = await execute(variables, (progress) => write({ ...read(), progress }));
        write({ ...read(), status: "success", data });
      } catch (error) {
        write({ ...read(), status: "error", error });
      }
    },
  };
}
