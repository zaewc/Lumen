import type {
  AssessProgressView,
  CandidateView,
  PlanView,
  QuarantineReportView,
  ScanProgressView,
  ScanView,
} from "@lumen/api-client";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { createContext, type ReactNode, useContext, useMemo, useRef, useState } from "react";
import { QUERY_KEYS, useSessionOperation, useTransport } from "@/shared/api";

/** Where the assessment of the latest scan is. */
export type AssessmentStatus = "pending" | "error" | "success";

/**
 * This session's scan, its assessment and the clean-up that followed, shared
 * by every page (Overview shows a summary, Clean up the details), so a scan
 * started on one page goes on while the user looks at another.
 */
export interface ScanSession {
  /** A scan is running. */
  readonly scanning: boolean;
  /** The latest totals of the running scan, if it has reported any. */
  readonly progress: ScanProgressView | null;
  /** Why the last scan failed, or `null`. */
  readonly scanError: unknown;
  /** The completed scan, until it is put aside. */
  readonly scan: ScanView | null;
  /** The assessment of that scan: it starts by itself and moves nothing. */
  readonly assessment: AssessmentStatus;
  readonly candidates: readonly CandidateView[] | null;
  readonly assessmentError: unknown;
  readonly assessmentProgress: AssessProgressView | null;
  /** What the last clean-up did, until a new scan starts. */
  readonly report: QuarantineReportView | null;
  /** Scans these roots (none: the whole disk), read-only. */
  readonly start: (roots: string[]) => void;
  /** Scans the same roots as the last scan again (the whole disk if none). */
  readonly rescan: () => void;
  /** Puts the scan and its results aside, back to the start. */
  readonly startOver: () => void;
  /** Confirmed clean-up continues even when its page is no longer open. */
  readonly cleaning: boolean;
  readonly cleanUpError: unknown;
  readonly execute: (plan: PlanView) => void;
  readonly resetCleanUp: () => void;
}

const ScanSessionContext = createContext<ScanSession | null>(null);

/** Keeps this session's scan for every page below it. */
export function ScanSessionProvider({ children }: { readonly children: ReactNode }) {
  const transport = useTransport();
  const queries = useQueryClient();
  const [progress, setProgress] = useState<ScanProgressView | null>(null);
  const [assessmentProgress, setAssessmentProgress] = useState<AssessProgressView | null>(null);
  const generation = useRef(0);
  const [report, setReport] = useState<QuarantineReportView | null>(null);
  const [lastRoots, setLastRoots] = useState<string[]>([]);
  const scan = useMutation({
    mutationFn: (roots: string[]) => {
      setProgress(null);
      generation.current += 1;
      setAssessmentProgress(null);
      return transport.scan({ roots }, setProgress);
    },
  });
  const scanId = scan.data?.scan;
  const assessment = useQuery({
    queryKey: [...QUERY_KEYS.assessment, scanId],
    queryFn: () => {
      const current = generation.current;
      setAssessmentProgress(null);
      return transport.assess((next) => {
        if (generation.current === current) setAssessmentProgress(next);
      });
    },
    enabled: scanId !== undefined,
    retry: false,
    staleTime: Number.POSITIVE_INFINITY,
    refetchOnWindowFocus: false,
  });

  const { mutate, reset } = scan;
  const execution = useSessionOperation(
    ["session-operation", "clean-up"],
    async (confirmed: PlanView) => {
      const finished = await transport.execute({
        plan: confirmed.plan,
        confirmation: confirmed.hash,
      });
      setReport(finished);
      reset();
      queries.removeQueries({ queryKey: QUERY_KEYS.assessment });
      await queries.invalidateQueries({ queryKey: QUERY_KEYS.held });
      await queries.invalidateQueries({ queryKey: QUERY_KEYS.disk });
      return finished;
    },
  );
  const cleaning = execution.status === "pending";
  const value = useMemo<ScanSession>(
    () => ({
      scanning: scan.isPending,
      progress,
      scanError: scan.error,
      scan: scan.data ?? null,
      assessment: assessment.status,
      candidates: assessment.data ?? null,
      assessmentError: assessment.error,
      assessmentProgress,
      report,
      cleaning,
      cleanUpError: execution.error,
      execute: (confirmed) => {
        void execution.run(confirmed);
      },
      resetCleanUp: execution.reset,
      start: (roots) => {
        if (cleaning || scan.isPending) {
          return;
        }
        execution.reset();
        setReport(null);
        setLastRoots(roots);
        mutate(roots);
      },
      rescan: () => {
        if (cleaning || scan.isPending) {
          return;
        }
        execution.reset();
        setReport(null);
        mutate(lastRoots);
      },
      startOver: () => {
        if (cleaning || scan.isPending) {
          return;
        }
        execution.reset();
        setReport(null);
        setProgress(null);
        generation.current += 1;
        setAssessmentProgress(null);
        reset();
      },
    }),
    [
      scan.isPending,
      scan.error,
      scan.data,
      progress,
      assessment.status,
      assessment.data,
      assessment.error,
      assessmentProgress,
      report,
      lastRoots,
      mutate,
      reset,
      queries,
      cleaning,
      execution.error,
      execution.run,
      execution.reset,
    ],
  );
  return <ScanSessionContext value={value}>{children}</ScanSessionContext>;
}

/** This session's scan; only valid inside `ScanSessionProvider`. */
export function useScanSession(): ScanSession {
  const session = useContext(ScanSessionContext);
  if (session === null) {
    throw new Error("useScanSession outside ScanSessionProvider");
  }
  return session;
}
