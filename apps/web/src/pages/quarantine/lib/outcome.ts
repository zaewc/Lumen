import type { FinalizeView, NotRestoredView, RemovedView, RestoreView } from "@lumen/api-client";
import type { Messages } from "@/shared/i18n";

/** The last action on a held item, kept as data so it follows a language switch. */
export type Done =
  | { readonly action: "restore"; readonly outcome: RestoreView }
  | { readonly action: "finalize"; readonly outcome: FinalizeView };

/** The last action and the item it was about (its original path, if known). */
export interface LastAction {
  readonly original: string | null;
  readonly done: Done;
}

function notRestoredText(m: Messages, reason: NotRestoredView): string {
  switch (reason) {
    case "name_taken":
      return m.restore.nameTaken;
    case "origin_unreachable":
      return m.restore.originUnreachable;
    case "item_changed":
      return m.restore.itemChanged;
    case "failed":
      return m.restore.moveFailed;
  }
}

/** What a restore did, in words. */
export function restoreText(m: Messages, outcome: RestoreView): string {
  switch (outcome.kind) {
    case "restored": {
      const parts = [m.restore.restored];
      if (outcome.parent_replaced) {
        parts.push(m.restore.parentReplaced);
      }
      if (!outcome.verified) {
        parts.push(m.restore.unverified);
      }
      return parts.join(" ");
    }
    case "not_restored":
      return m.restore.notRestored(notRestoredText(m, outcome.reason));
    case "not_restorable":
      return m.restore.notRestorable;
  }
}

function removedText(m: Messages, removed: RemovedView): string {
  switch (removed) {
    case "nothing":
      return m.finalize.removedNothing;
    case "partly":
      return m.finalize.removedPartly;
    case "item":
      return m.finalize.removedItem;
  }
}

/** What a finalization did, in words. */
export function finalizeText(m: Messages, outcome: FinalizeView): string {
  switch (outcome.kind) {
    case "finalized":
      return m.finalize.finalized;
    case "found_at_source":
      return m.finalize.foundAtSource;
    case "not_found":
      return m.finalize.notFound;
    case "failed":
      return m.finalize.failed(removedText(m, outcome.removed));
    case "maybe_in_use":
      return m.finalize.maybeInUse;
    case "not_finalizable":
      return m.finalize.notFinalizable;
  }
}

/** Whole days between `since` (RFC 3339) and `now`. */
export function daysHeld(since: string, now: Date): number {
  return Math.max(0, Math.floor((now.getTime() - Date.parse(since)) / 86_400_000));
}

/** What the last action did, in words. */
export function doneText(m: Messages, done: Done): string {
  switch (done.action) {
    case "restore":
      return restoreText(m, done.outcome);
    case "finalize":
      return finalizeText(m, done.outcome);
  }
}

/** Whether the last action did all it was asked to (else it needs a look). */
export function doneWell(done: Done): boolean {
  switch (done.action) {
    case "restore":
      return done.outcome.kind === "restored" && done.outcome.verified;
    case "finalize":
      return done.outcome.kind === "finalized";
  }
}
