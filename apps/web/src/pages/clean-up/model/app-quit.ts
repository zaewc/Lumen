import type { QuitView, UninstallPreviewView } from "@lumen/api-client";

/** How long the page keeps checking after an app was asked to quit. */
export const QUIT_WAIT_MS = 15_000;

/** How often it checks meanwhile. */
export const QUIT_CHECK_MS = 1_500;

/**
 * Where asking a running app to quit stands (roadmap 16.3a):
 * - `none`: nothing to offer; the app is not refused for running, or
 *   something else also keeps it, which quitting would not change;
 * - `offer`: running is the only reason, so offer to quit it;
 * - `waiting`: it was asked and may be asking to save; checking again;
 * - `still_running`: it was asked but still runs after the wait;
 * - `itself`: it is Lumen;
 * - `failed`: the request could not be sent.
 */
export type QuitStage = "none" | "offer" | "waiting" | "still_running" | "itself" | "failed";

/** Whether the preview refuses the app only because it runs. */
export function onlyRunning(preview: UninstallPreviewView | null): boolean {
  return (
    preview?.kind === "refused" &&
    preview.reasons.length > 0 &&
    preview.reasons.every((reason) => reason === "running")
  );
}

/** Whether the preview refuses the app because it runs, or may run. */
export function mayBeRunning(preview: UninstallPreviewView | null): boolean {
  return (
    preview?.kind === "refused" &&
    preview.reasons.some((reason) => reason === "running" || reason === "running_unknown")
  );
}

/**
 * The stage from the latest preview, the answer to the last quit request
 * (`null` if none was sent) and whether the wait after it is still on.
 */
export function quitStage(
  preview: UninstallPreviewView | null,
  answer: QuitView | null,
  waiting: boolean,
): QuitStage {
  if (!onlyRunning(preview)) return "none";
  if (answer === null) return "offer";
  switch (answer.kind) {
    case "asked":
    case "not_running":
      return waiting ? "waiting" : "still_running";
    case "itself":
      return "itself";
    case "failed":
    case "not_found":
    case "ambiguous":
    case "unsupported":
      return "failed";
  }
}
