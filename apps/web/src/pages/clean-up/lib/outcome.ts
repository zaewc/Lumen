import type { ItemResultView, LocationView, SkipView } from "@lumen/api-client";
import type { Messages } from "@/shared/i18n";

function skipText(m: Messages, reason: SkipView): string {
  switch (reason) {
    case "changed":
      return m.item.skipChanged;
    case "outside_roots":
      return m.item.skipOutsideRoots;
    case "now_keep":
      return m.item.skipNowKeep;
    case "now_review":
      return m.item.skipNowReview;
    case "reasons_changed":
      return m.item.skipReasonsChanged;
    case "not_prepared":
      return m.item.skipNotPrepared;
    case "manifest":
      return m.item.skipManifest;
  }
}

function locationText(m: Messages, left: LocationView): string {
  switch (left) {
    case "source":
      return m.item.leftSource;
    case "quarantine":
      return m.item.leftQuarantine;
    case "gone":
      return m.item.leftGone;
  }
}

/** One item's outcome, in words. The failure reason is the core's own code. */
export function outcomeText(m: Messages, result: ItemResultView): string {
  switch (result.kind) {
    case "quarantined":
      return m.item.quarantined;
    case "skipped":
      return m.item.skipped(skipText(m, result.reason));
    case "failed":
      return m.item.failed(result.reason, locationText(m, result.left));
  }
}
