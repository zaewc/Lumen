import type { FailureView, ItemResultView, LocationView, SkipView } from "@lumen/api-client";
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

function failureText(m: Messages, reason: FailureView): string {
  switch (reason) {
    case "identity_mismatch":
      return m.item.failIdentityMismatch;
    case "in_use":
      return m.item.failInUse;
    case "volume_unsupported":
      return m.item.failVolumeUnsupported;
    case "move_failed":
      return m.item.failMoveFailed;
    case "verification_failed":
      return m.item.failVerificationFailed;
    case "other":
      return m.item.failOther;
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

/** One item's outcome, in words. */
export function outcomeText(m: Messages, result: ItemResultView): string {
  switch (result.kind) {
    case "quarantined":
      return m.item.quarantined;
    case "skipped":
      return m.item.skipped(skipText(m, result.reason));
    case "failed":
      return m.item.failed(failureText(m, result.reason), locationText(m, result.left));
  }
}
