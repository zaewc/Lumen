import type { UpdateView } from "@lumen/api-client";
import { focusManager, QueryClient, QueryObserver } from "@tanstack/react-query";
import { describe, expect, it, vi } from "vitest";
import { updateOptions } from "./options";

const current: UpdateView = { current: "0.8.0", available: null, installable: true };

describe("automatic update checks", () => {
  it("shares the startup check and refreshes on return only after results are stale", async () => {
    const cache = new QueryClient();
    const first = Promise.withResolvers<UpdateView>();
    const next = Promise.withResolvers<UpdateView>();
    const check = vi.fn().mockReturnValueOnce(first.promise).mockReturnValueOnce(next.promise);
    const options = updateOptions(check);
    cache.mount();
    const a = new QueryObserver(cache, options);
    const b = new QueryObserver(cache, options);
    const subscriptions = [a.subscribe(() => {}), b.subscribe(() => {})];
    try {
      expect(check).toHaveBeenCalledTimes(1);
      first.resolve(current);
      await vi.waitFor(() => expect(a.getCurrentResult().data).toEqual(current));
      focusManager.setFocused(false);
      focusManager.setFocused(true);
      await new Promise((resolve) => setTimeout(resolve, 0));
      expect(check).toHaveBeenCalledTimes(1);
      cache.setQueryData(options.queryKey, current, {
        updatedAt: Date.now() - 24 * 60 * 60 * 1000,
      });
      focusManager.setFocused(false);
      focusManager.setFocused(true);
      await vi.waitFor(() => expect(check).toHaveBeenCalledTimes(2));
      next.resolve({ ...current, available: "0.8.1" });
      await vi.waitFor(() => expect(b.getCurrentResult().data?.available).toBe("0.8.1"));
    } finally {
      for (const unsubscribe of subscriptions) unsubscribe();
      cache.unmount();
      cache.clear();
      focusManager.setFocused(undefined);
    }
  });

  it("does not check an unsupported host and does not retry a failed startup check", async () => {
    const cache = new QueryClient();
    const browser = new QueryObserver(cache, updateOptions(undefined));
    const stopBrowser = browser.subscribe(() => {});
    try {
      expect(browser.getCurrentResult().fetchStatus).toBe("idle");
    } finally {
      stopBrowser();
      cache.clear();
    }
    const check = vi.fn().mockRejectedValue(new Error("offline"));
    const desktop = new QueryObserver(cache, updateOptions(check));
    const stopDesktop = desktop.subscribe(() => {});
    try {
      await vi.waitFor(() => expect(desktop.getCurrentResult().isError).toBe(true));
      expect(desktop.getCurrentResult().data).toBeUndefined();
      expect(check).toHaveBeenCalledTimes(1);
      expect(desktop.getCurrentResult().failureCount).toBe(1);
    } finally {
      stopDesktop();
      cache.clear();
    }
  });
});
