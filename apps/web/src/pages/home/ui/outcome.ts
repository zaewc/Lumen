import type { FinalizeView, NotRestoredView, RemovedView, RestoreView } from "@lumen/api-client";

function notRestoredText(reason: NotRestoredView): string {
  switch (reason) {
    case "name_taken":
      return "something now has its name at the original place; move that away and try again";
    case "origin_unreachable":
      return "its original folder cannot be reached safely";
    case "item_changed":
      return "the quarantined item changed";
    case "failed":
      return "the move back failed";
  }
}

/** What a restore did, in words. */
export function restoreText(outcome: RestoreView): string {
  switch (outcome.kind) {
    case "restored": {
      const parts = ["Restored to its original place."];
      if (outcome.parent_replaced) {
        parts.push("Its folder had been recreated since.");
      }
      if (!outcome.verified) {
        parts.push("It is back but did not match its record exactly; check it.");
      }
      return parts.join(" ");
    }
    case "not_restored":
      return `Not restored: ${notRestoredText(outcome.reason)}. It stays in quarantine.`;
    case "not_restorable":
      return "This item cannot be restored in its current state.";
  }
}

function removedText(removed: RemovedView): string {
  switch (removed) {
    case "nothing":
      return "nothing was removed";
    case "partly":
      return "part of it was removed; try again to finish";
    case "item":
      return "the item is gone, only its record remains";
  }
}

/** What a finalization did, in words. */
export function finalizeText(outcome: FinalizeView): string {
  switch (outcome.kind) {
    case "finalized":
      return "Removed for good.";
    case "found_at_source":
      return "It is back at its original place already; nothing was removed.";
    case "not_found":
      return "It is not in quarantine; nothing was removed.";
    case "failed":
      return `Could not remove it completely: ${removedText(outcome.removed)}.`;
    case "maybe_in_use":
      return "It may be in use, or that cannot be checked; nothing was removed. Close the programs that use it and try again.";
    case "not_finalizable":
      return "Only retained items can be removed for good.";
  }
}

/** Whole days between `since` (RFC 3339) and `now`. */
export function daysHeld(since: string, now: Date): number {
  return Math.max(0, Math.floor((now.getTime() - Date.parse(since)) / 86_400_000));
}
