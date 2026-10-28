import { QueryClient, QueryObserver } from "@tanstack/react-query";
import { describe, expect, it } from "vitest";
import { sessionOperation } from "./operation";

describe("session operation lifetime", () => {
  it("keeps running with no view, refuses overlap and returns its result to a new view", async () => {
    const cache = new QueryClient({ defaultOptions: { queries: { gcTime: 0 } } });
    const key = ["confirmed-operation"];
    const gate = Promise.withResolvers<number>();
    let calls = 0;
    const start = sessionOperation(cache, key, async () => {
      calls += 1;
      return gate.promise;
    });
    const unsubscribe = new QueryObserver(cache, { queryKey: key, enabled: false }).subscribe(
      () => {},
    );
    const running = start.run("confirmed plan");
    unsubscribe();
    await new Promise((resolve) => setTimeout(resolve, 10));
    const reopened = sessionOperation(cache, key, async () => 99);
    expect(reopened.read().status).toBe("pending");
    reopened.reset();
    await reopened.run("duplicate");
    expect(reopened.read().status).toBe("pending");
    expect(calls).toBe(1);
    gate.resolve(42);
    await running;
    expect(reopened.read()).toMatchObject({
      status: "success",
      variables: "confirmed plan",
      data: 42,
    });
    cache.clear();
  });

  it("retains progress and failures after navigation, and retries only on another explicit run", async () => {
    const cache = new QueryClient();
    const key = ["held-action"];
    const gate = Promise.withResolvers<number>();
    const action = sessionOperation<string, number, number>(cache, key, async (_input, report) => {
      report(1);
      return gate.promise;
    });
    const running = action.run("restore selected");
    expect(action.read()).toMatchObject({ status: "pending", progress: 1 });
    const failure = new Error("temporary failure");
    gate.reject(failure);
    await running;
    const reopened = sessionOperation(cache, key, async () => 2);
    expect(reopened.read()).toMatchObject({ status: "error", progress: 1, error: failure });
    await reopened.run("restore selected again");
    expect(reopened.read()).toMatchObject({
      status: "success",
      data: 2,
      error: null,
      progress: null,
    });
    reopened.reset();
    expect(reopened.read().status).toBe("idle");
    cache.clear();
  });
});
