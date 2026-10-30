import type { UpdateView } from "@lumen/api-client";
import { QueryClient, QueryObserver } from "@tanstack/react-query";
import { describe, expect, it, vi } from "vitest";
import { QUERY_KEYS } from "@/shared/api";
import { followUpdateFocus } from "./updateFocus";

const current: UpdateView = { current: "0.8.0", available: null, installable: true };

describe("update window focus", () => {
  it("checks on native focus while visible, sharing overlapping DOM focus checks", async () => {
    const queries = new QueryClient();
    const target = new EventTarget();
    const next = Promise.withResolvers<UpdateView>();
    const check = vi.fn().mockResolvedValueOnce(current).mockReturnValueOnce(next.promise);
    const observer = new QueryObserver(queries, {
      queryKey: QUERY_KEYS.update,
      queryFn: check,
      staleTime: 6 * 60 * 60 * 1000,
    });
    const unsubscribe = observer.subscribe(() => {});
    const stop = followUpdateFocus(queries, target);
    const focus = () => target.dispatchEvent(new Event("lumen:window-focus"));
    const clock = vi.spyOn(Date, "now");
    try {
      await vi.waitFor(() => expect(observer.getCurrentResult().data).toEqual(current));
      focus();
      target.dispatchEvent(new Event("focus"));
      expect(check).toHaveBeenCalledTimes(1);
      clock.mockReturnValue(Date.now() + 7 * 60 * 60 * 1000);
      expect(observer.getCurrentResult().isStale).toBe(false);
      target.dispatchEvent(new Event("blur"));
      expect(check).toHaveBeenCalledTimes(1);
      focus();
      target.dispatchEvent(new Event("focus"));
      focus();
      expect(check).toHaveBeenCalledTimes(2);
      next.resolve({ ...current, available: "0.8.1" });
      await vi.waitFor(() => expect(observer.getCurrentResult().data?.available).toBe("0.8.1"));
      focus();
      expect(check).toHaveBeenCalledTimes(2);
      stop();
      queries.setQueryData(QUERY_KEYS.update, current, { updatedAt: 1 });
      focus();
      target.dispatchEvent(new Event("focus"));
      expect(check).toHaveBeenCalledTimes(2);
    } finally {
      stop();
      unsubscribe();
      queries.clear();
      clock.mockRestore();
    }
  });

  it("does not refetch unrelated, inactive or disabled queries", async () => {
    const queries = new QueryClient();
    const target = new EventTarget();
    const other = vi.fn().mockResolvedValue("disk");
    const disabled = vi.fn();
    const disk = new QueryObserver(queries, { queryKey: ["disk"], queryFn: other });
    const update = new QueryObserver(queries, {
      queryKey: QUERY_KEYS.update,
      queryFn: disabled,
      enabled: false,
    });
    const unsubscribeDisk = disk.subscribe(() => {});
    const unsubscribeUpdate = update.subscribe(() => {});
    const stop = followUpdateFocus(queries, target);
    try {
      await vi.waitFor(() => expect(disk.getCurrentResult().data).toBe("disk"));
      target.dispatchEvent(new Event("lumen:window-focus"));
      target.dispatchEvent(new Event("focus"));
      await new Promise((resolve) => setTimeout(resolve, 0));
      expect(other).toHaveBeenCalledTimes(1);
      expect(disabled).not.toHaveBeenCalled();
      unsubscribeUpdate();
      queries.setQueryDefaults(QUERY_KEYS.update, { queryFn: disabled });
      queries.setQueryData(QUERY_KEYS.update, current, { updatedAt: 1 });
      target.dispatchEvent(new Event("lumen:window-focus"));
      expect(disabled).not.toHaveBeenCalled();
    } finally {
      stop();
      unsubscribeDisk();
      unsubscribeUpdate();
      queries.clear();
    }
  });
});
