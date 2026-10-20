/** Where the clean-up flow is: one card on the page for each. */
export type CleanUpStep = "start" | "scanning" | "deciding" | "decideFailed" | "results" | "done";

export interface FlowState {
  /** A scan is running. */
  readonly scanning: boolean;
  /** A scan finished and has not been put aside. */
  readonly scanned: boolean;
  /** The assessment of that scan. */
  readonly assessment: "pending" | "error" | "success";
  /** A clean-up ran; its report is shown. */
  readonly done: boolean;
}

/** The step to show. The assessment follows the scan on its own. */
export function cleanUpStep(state: FlowState): CleanUpStep {
  if (state.done) {
    return "done";
  }
  if (state.scanning) {
    return "scanning";
  }
  if (!state.scanned) {
    return "start";
  }
  switch (state.assessment) {
    case "pending":
      return "deciding";
    case "error":
      return "decideFailed";
    case "success":
      return "results";
  }
}
