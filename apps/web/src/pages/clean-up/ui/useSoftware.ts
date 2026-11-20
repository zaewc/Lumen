import type { SoftwareView } from "@lumen/api-client";
import { useQuery } from "@tanstack/react-query";
import { useAiSettings } from "@/entities/ai-review";
import { useTransport } from "@/shared/api";

/** What the host says is installed, and how reading it goes. */
export interface Software {
  /** The list, with AI notes once AI review explained it; `undefined` until read. */
  readonly list: SoftwareView | undefined;
  /** The host can read the list (the desktop app). */
  readonly available: boolean;
  readonly reading: boolean;
  readonly explaining: boolean;
  readonly error: unknown;
  readonly failed: boolean;
  /** Reads the list again, then explains it again while AI review is on. */
  readonly readAgain: () => void;
}

/**
 * The installed software for the Storage list (ADR-0060, ADR-0069): read
 * when the page needs it, and, while AI review is on, explained once it is
 * in (ADR-0061); the list shows first and the notes fill in when they come.
 */
export function useSoftware(): Software {
  const transport = useTransport();
  const ask = transport.software;
  const explain = transport.explainSoftware;
  const aiOn = useAiSettings().data?.enabled === true;
  const software = useQuery({
    queryKey: ["software"],
    queryFn: () => (ask === undefined ? Promise.reject(new Error("unsupported")) : ask()),
    enabled: ask !== undefined,
    retry: false,
  });
  const explained = useQuery({
    queryKey: ["software", "explained"],
    queryFn: () => (explain === undefined ? Promise.reject(new Error("unsupported")) : explain()),
    enabled: explain !== undefined && aiOn && software.isSuccess,
    retry: false,
    refetchOnWindowFocus: false,
  });
  // The explained list only while AI review is on, its last read succeeded
  // and it is no older than the inventory: a failed or stale explanation
  // never hides a newer read.
  const explainedFresh =
    aiOn && explained.status === "success" && explained.dataUpdatedAt >= software.dataUpdatedAt;
  return {
    list: explainedFresh ? explained.data : software.data,
    available: ask !== undefined,
    reading: software.isFetching,
    explaining: explained.isFetching,
    error: software.error,
    failed: software.isError,
    readAgain: () => {
      void software.refetch().then(() => {
        if (explain !== undefined && aiOn) void explained.refetch();
      });
    },
  };
}
