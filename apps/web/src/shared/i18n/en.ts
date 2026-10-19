import { countFormat } from "./format";
import type { RuleId } from "./rules";

const count = countFormat("en");

/** The English catalog. Its shape is the shape of every catalog. */
export const en = {
  language: "Language",
  nav: {
    quarantine: "Quarantine",
    scan: "Scan",
    plan: "Plan",
  },
  home: {
    tagline: "Storage cleanup that keeps everything reversible until you decide otherwise.",
    heading: "In quarantine",
    spaceNote:
      "Quarantined items still use space; it is freed only when an item is removed for good.",
    loading: "Loading…",
    empty: "Nothing is held.",
  },
  held: {
    unknownPath: "(unknown path)",
    retained: "Retained",
    heldAfterFailure: "Held after a failed move",
    age: (days: number) => `${count(days)} day(s)`,
    restore: "Restore",
    removeForGoodAsk: "Remove for good…",
    removeForGoodWarning: "This cannot be undone. It is checked for use first.",
    removeForGood: "Remove for good",
    cancel: "Cancel",
  },
  restore: {
    restored: "Restored to its original place.",
    parentReplaced: "Its folder had been recreated since.",
    unverified: "It is back but did not match its record exactly; check it.",
    notRestored: (why: string) => `Not restored: ${why}. It stays in quarantine.`,
    nameTaken: "something now has its name at the original place; move that away and try again",
    originUnreachable: "its original folder cannot be reached safely",
    itemChanged: "the quarantined item changed",
    moveFailed: "the move back failed",
    notRestorable: "This item cannot be restored in its current state.",
  },
  finalize: {
    finalized: "Removed for good.",
    foundAtSource: "It is back at its original place already; nothing was removed.",
    notFound: "It is not in quarantine; nothing was removed.",
    failed: (what: string) => `Could not remove it completely: ${what}.`,
    removedNothing: "nothing was removed",
    removedPartly: "part of it was removed; try again to finish",
    removedItem: "the item is gone, only its record remains",
    maybeInUse:
      "It may be in use, or that cannot be checked; nothing was removed. Close the programs that use it and try again.",
    notFinalizable: "Only retained items can be removed for good.",
  },
  plan: {
    title: "Plan",
    intro:
      "A plan holds every item the policy proposes for quarantine in the latest scan. Making it moves nothing.",
    planning: "Planning…",
    make: "Make a plan",
    summary: (items: number, size: string) => `${count(items)} item(s), ${size}`,
    made: (createdAt: string) => `Made ${createdAt}; valid for 24 hours. Plan hash`,
    confirm:
      "Move exactly these items into quarantine. Each is checked again first, and anything that changed stays where it is. Nothing is deleted: every item can be restored until I remove it for good.",
    quarantining: "Quarantining…",
    execute: (items: number) => `Quarantine ${count(items)} item(s)`,
    moved: (size: string) => `${size} now in quarantine`,
    freedNote: "Space is freed only when items are removed for good.",
  },
  item: {
    quarantined: "Moved into quarantine (restorable).",
    skipped: (why: string) => `Not moved: ${why}.`,
    failed: (reason: string, where: string) => `The move failed (${reason}); ${where}.`,
    skipChanged: "it changed or can no longer be observed",
    skipOutsideRoots: "it is outside the scanned folders",
    skipNowKeep: "the policy now keeps it",
    skipNowReview: "the policy now wants you to review it",
    skipReasonsChanged: "it would now be proposed for different reasons",
    skipNotPrepared: "it could not be recorded for a safe move",
    skipManifest: "its record could not be written",
    leftSource: "it is untouched in its place",
    leftQuarantine: "it is in quarantine and can be restored",
    leftGone: "it is gone",
  },
  scan: {
    title: "Scan",
    rootsLabel: "Folders to scan, one per line",
    readOnly: "Scanning only reads; nothing is changed.",
    scanning: "Scanning…",
    submit: "Scan",
    wholeDisk: "Scan the whole disk",
    wholeDiskHint:
      "Reads every folder on the startup disk, one disk at a time; this can take a few minutes. Your system may ask to allow access to some folders; folders you do not allow are reported as unread, never as empty.",
    found: "What was found",
    partial: "The scan stopped early; results are partial.",
    assessment: "Assessment",
    nothingMoved: "Nothing has been moved.",
    deciding: "Deciding…",
  },
  root: {
    files: "Files",
    fileCount: (files: string) => count(files),
    size: "Size",
    sizeValue: (logical: string, allocated: string) => `${logical} (allocated ${allocated})`,
    reclaimable: "Reclaimable",
    atLeast: (size: string) => `${size} at least`,
    complete: "Everything below it was read.",
    incomplete: (denied: string, failed: string, notScanned: string) =>
      `Not everything could be read (${count(denied)} denied, ${count(failed)} failed, ${count(notScanned)} not scanned). The sizes are lower bounds, and nothing unread is treated as empty.`,
  },
  candidates: {
    none: "No candidates were found in the latest scan.",
    quarantine: "Proposed for quarantine",
    review: "Needs your review",
    keep: "Kept",
    group: (label: string, items: number) => `${label} (${count(items)})`,
  },
  whatIs: (term: string) => `What does “${term}” mean?`,
  glossary: {
    scan: {
      title: "Scan",
      body: "A scan reads the folders you choose and measures what is inside them. It only reads: nothing is moved, changed or deleted. It looks at file details such as names and sizes, never opens files, and never downloads files that live only in the cloud.",
    },
    coverage: {
      title: "Coverage",
      body: "Coverage says whether Lumen could read everything inside a folder. When everything was read, the sizes are complete. When some items were denied, failed or not scanned, the sizes shown are lower bounds. Lumen never treats what it could not read as empty, and never proposes removing what it could not check.",
    },
    reclaimable: {
      title: "Reclaimable",
      body: "Reclaimable is the space you would get back if items were removed for good; space other files still share (hard links, clones, snapshots) is not counted. Nothing is freed while items are only in quarantine. “At least” means part of the folder could not be read, so the real figure may be larger. Size is how much data the files hold; allocated is how much of the disk they occupy.",
    },
    verdicts: {
      title: "Verdicts",
      body: "Lumen decides about every item with a fixed safety policy, not by guessing. Each item is Kept, Needs your review, or Proposed for quarantine, with its reasons listed below it. When Lumen is unsure, it keeps the item or asks you, and nothing is moved until you confirm a plan.",
    },
    keep: {
      title: "Kept",
      body: "Lumen will not touch this item. It may be protected by the system, in use, synced with a cloud service or not fully readable, or Lumen knows it is needed. Kept items are never moved.",
    },
    review: {
      title: "Needs your review",
      body: "Lumen cannot decide safely on its own, for example because it does not recognise the item or the item holds an application's own data. It is never moved automatically. Read the reasons and decide for yourself.",
    },
    proposed: {
      title: "Proposed for quarantine",
      body: "The policy found this item safe to set aside, for example a cache that its tool or application re-creates. Proposing moves nothing: the item only goes into a plan that you read and confirm. Even then it goes to quarantine, where it can be restored.",
    },
    plan: {
      title: "Plan",
      body: "A plan is a frozen list of the items proposed for quarantine in the latest scan, sealed with a hash so that exactly what you read is what runs. It is valid for 24 hours, and making it moves nothing. When you confirm it, each item is checked again first; anything that changed is skipped and stays where it is.",
    },
    quarantine: {
      title: "Quarantine",
      body: "Quarantine moves items into a private Lumen folder instead of deleting them. Every quarantined item can be restored to its original place. Quarantined items still use disk space until you remove them for good.",
    },
    finalize: {
      title: "Remove for good",
      body: "Removing for good deletes a quarantined item permanently and frees its space. It is the only step that cannot be undone. Lumen first checks that nothing is using the item, and removes nothing if it is in use or that cannot be checked. Only retained items can be removed for good.",
    },
    restore: {
      title: "Restore",
      body: "Restoring moves an item from quarantine back to its original place, under its original name. Lumen never overwrites anything: if something now has that name, or the original folder cannot be reached safely, the item stays in quarantine and Lumen tells you why.",
    },
    held: {
      title: "Held items",
      body: "Retained means the item was moved into quarantine and checked against its record; it waits there until you restore it or remove it for good. Held after a failed move means something went wrong during the move, so the item was kept safely in quarantine without that check. Such items can be restored, but not removed for good.",
    },
  },
  reasons: {
    location: (name: string) => `Kept: it is in a protected location (${name}).`,
    rules: {
      "default.review": "Review: Lumen does not recognise this item.",
      "protect.os-protected": "Kept: the operating system protects this item.",
      "protect.system-file": "Kept: Windows marks this item as a system file.",
      "protect.cloud-backed":
        "Kept: it is synced with a cloud service, so removing it here could remove it everywhere.",
      "protect.in-use": "Kept: a running process is using it.",
      "protect.in-use-partial":
        "Kept: no process Lumen can see uses it, but not every process could be checked.",
      "protect.in-use-unknown": "Kept: Lumen has not checked whether anything uses it.",
      "protect.not-observed": "Kept: the scan has not observed this item.",
      "protect.incomplete-scan": "Kept: part of it could not be read, so its contents are unknown.",
      "protect.location-data": "Kept: Lumen could not check its list of protected locations.",
      "protect.location-context": "Kept: Lumen could not check its list of protected locations.",
      "eligible.quarantine-unsupported":
        "Review only: this device cannot quarantine it reversibly.",
      "eligible.frees-nothing":
        "Review only: removing it would free no space now (hard links, clones, snapshots or cloud-only data).",
      "eligible.object-kind": "Review only: it is a link or special file, not data.",
      "eligible.in-use-partial":
        "No process Lumen can see uses it; system processes could not be checked, so Lumen checks again just before moving it.",
      "classify.devkb-regenerable": "Quarantine proposed: the tool that made it can regenerate it.",
      "classify.devkb-tool-action":
        "Review: only the tool's own cleanup command should remove it, and that cannot be undone.",
      "classify.devkb-not-regenerable": "Review: it cannot be regenerated.",
      "classify.devkb-self-managed": "Kept: the tool that made it cleans it up automatically.",
      "classify.devkb-unknown": "Review: the knowledge-base entry that matched it is unavailable.",
      "classify.vendor-declared-cache": "Review: its vendor declares its contents disposable.",
      "classify.app-cache":
        "Quarantine proposed: it is an application's cache, which the application re-creates.",
      "classify.app-state": "Review: it holds an application's own data, settings or documents.",
      "classify.orphan": "Review: the application it belongs to appears to be uninstalled.",
      "jev.demote": "Review: the optional AI review advised caution.",
    } satisfies { readonly [R in RuleId]: string },
  },
};

/** What every catalog provides: the same keys and message signatures as `en`. */
export type Messages = typeof en;

/** A concept the glossary explains. */
export type GlossaryTerm = keyof Messages["glossary"];
