/** One step of the first-launch setup (ADR-0067), in order. */
export type SetupStep = "ai" | "helper" | "disk";

/** What the host can offer besides the AI review question. */
export interface SetupOffers {
  /** The host can report and register an administrator helper. */
  readonly helper: boolean;
  /** A Mac whose host can open System Settings at Full Disk Access. */
  readonly disk: boolean;
}

/** The setup's steps: AI review always, then what the host can offer. */
export function setupSteps(offers: SetupOffers): readonly SetupStep[] {
  return [
    "ai",
    ...(offers.helper ? (["helper"] as const) : []),
    ...(offers.disk ? (["disk"] as const) : []),
  ];
}
