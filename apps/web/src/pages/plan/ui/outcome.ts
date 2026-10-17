import type { ItemResultView, LocationView, SkipView } from "@lumen/api-client";

function skipText(reason: SkipView): string {
  switch (reason) {
    case "changed":
      return "it changed or can no longer be observed";
    case "outside_roots":
      return "it is outside the scanned folders";
    case "now_keep":
      return "the policy now keeps it";
    case "now_review":
      return "the policy now wants you to review it";
    case "reasons_changed":
      return "it would now be proposed for different reasons";
    case "not_prepared":
      return "it could not be recorded for a safe move";
    case "manifest":
      return "its record could not be written";
  }
}

function locationText(left: LocationView): string {
  switch (left) {
    case "source":
      return "it is untouched in its place";
    case "quarantine":
      return "it is in quarantine and can be restored";
    case "gone":
      return "it is gone";
  }
}

/** One item's outcome, in words. */
export function outcomeText(result: ItemResultView): string {
  switch (result.kind) {
    case "quarantined":
      return "Moved into quarantine (restorable).";
    case "skipped":
      return `Not moved: ${skipText(result.reason)}.`;
    case "failed":
      return `The move failed (${result.reason}); ${locationText(result.left)}.`;
  }
}
