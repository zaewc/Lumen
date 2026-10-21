import { countFormat } from "./format";
import type { RuleId } from "./rules";

const count = countFormat("en");

/** "1 item", "1,204 items". */
const items = (n: number) => `${count(n)} ${n === 1 ? "item" : "items"}`;

/** The English catalog. Its shape is the shape of every catalog. */
export const en = {
  language: "Language",
  nav: {
    label: "Main",
    overview: "Overview",
    cleanUp: "Clean up",
    quarantine: "Quarantine",
    settings: "Settings",
    held: (n: number) => `${items(n)} in Quarantine`,
  },
  common: {
    loading: "Loading…",
    cancel: "Cancel",
    details: "Details",
    why: "Why?",
    scanAgain: "Scan again",
    items: (n: number) => items(n),
  },
  overview: {
    title: "Overview",
    disk: {
      title: "Startup disk",
      meter: "Disk usage",
      used: "Used",
      available: "Available",
      total: "Total",
      percentUsed: (percent: string) => `${percent}% used`,
      low: "Your disk is almost full",
      lowHint: "Less than 10% is free. Scan to see what can be cleaned up safely.",
      loadFailed: "Disk usage could not be read.",
    },
    session: {
      title: "Last scan",
      canFree: "Can be cleaned up",
      review: "Needs review",
      quarantine: "In Quarantine",
      atLeast: (size: string) => `at least ${size}`,
      scanning: "Scanning your disk…",
      deciding: "Deciding what is safe to clean up…",
      showProgress: "Show progress",
      cleaned: (n: number, size: string) => `${items(n)} (${size}) moved to Quarantine.`,
    },
    empty: {
      title: "Find space you can free safely",
      body: "Lumen reads your disk, decides what is safe to set aside, and asks before it moves anything. Everything can be restored.",
      scan: "Scan disk",
    },
  },
  start: {
    title: "Scan for space to free",
    promise: "Nothing is deleted without your confirmation, and everything can be restored.",
    wholeDisk: "Scan the whole disk",
    wholeDiskHint:
      "Lumen only reads your disk; this can take a few minutes. Your computer may ask whether Lumen may look into some folders.",
    chooseFolders: "Choose folders…",
    foldersLabel: "Folders to scan, one per line",
    scanFolders: "Scan these folders",
    failed: "The scan could not finish. Nothing was changed.",
  },
  ai: {
    title: "AI review",
    offIntro:
      "An AI model double-checks what Lumen proposes to clean up. It can only make Lumen more careful. It stays off until you turn it on.",
    onIntro: (service: string) =>
      `On (${service}). Before an item is proposed for clean-up, its details (never its contents) are sent for a second opinion. It can only make Lumen more careful.`,
    checking: "Checking whether AI review is on…",
    short: {
      on: "AI review on",
      off: "AI review off",
    },
    statusOn: (service: string) => `AI review: on (${service})`,
    statusOff: "AI review: off",
    loadFailed: "The AI review setting could not be loaded.",
    changeFailed: "The AI review setting could not be changed.",
    consent: {
      title: "Turn on AI review?",
      what: "For each item proposed for quarantine, Lumen sends its metadata: the path (with your home folder shown as ~), sizes, ages and evidence facts. File contents are never sent.",
      whereJeb:
        "It goes to your school's Jeb server (jeb.https.gsmsv.site) with your API key. The server's operator may keep the requests.",
      whereOpenRouter: "It goes to TypeSafe's Jev through OpenRouter with your API key.",
      cautious:
        "The AI can only make a decision more cautious. It never chooses what is moved or removed.",
      revocable:
        "AI review then stays on for every scan until you turn it off, which you can do at any time.",
      keyLabel: "Jeb API key",
      keyHint: "Kept in your system's secure storage and never shown again.",
      keyInvalid: "Paste the whole key: 1–256 characters, no spaces.",
      confirm: "Turn on",
      turningOn: "Turning on…",
    },
  },
  scanning: {
    title: "Scanning…",
    deciding: "Deciding…",
    decidingHint: "Lumen is applying its safety rules to what it found.",
    readOnly: "Lumen is only reading; nothing is changed.",
    progress: "Scan progress",
    stepScan: "Scan",
    stepDecide: "Decide",
    items: "Items found",
    size: "Size on disk",
    unreadable: "Unreadable folders",
    reading: "Reading",
    failed: "Lumen could not decide about what it found. Nothing was changed.",
  },
  results: {
    canFree: "You can free up",
    proposedCount: (n: number) => `${items(n)} can be cleaned up safely`,
    nothing:
      "Nothing needs cleaning up right now. Lumen only suggests items it is sure are safe to set aside.",
    cleanUp: "Clean up",
    preparing: "Preparing…",
    planFailed: "Lumen could not prepare the clean-up. Nothing was moved.",
    found: "What Lumen found",
    proposed: "Can be cleaned up",
    review: "Needs your review",
    reviewHint:
      "Lumen is not sure about these, so it leaves them alone. Look through them yourself if you like.",
    keep: "Kept",
    keepHint: "Lumen will not touch these.",
    section: (n: number, size: string) => `${items(n)} · ${size}`,
    partial: "The scan stopped early, so these results are incomplete.",
    outOfSpace:
      "The scan stopped because the disk is almost full (Lumen always leaves 2 GB free), so these results are incomplete. Free some space and scan again.",
  },
  toolCommand: {
    title: (tool: string) => `${tool} can clean this up itself`,
    copy: "Copy",
    copied: "Copied",
    copyFailed: "Could not copy. Select the command to copy it yourself.",
    note: "Lumen does not run this command. Check what it removes first.",
  },
  coverage: {
    incomplete:
      "Some folders could not be read, so Lumen left them alone. The real sizes may be larger than shown.",
    fullDiskAccess:
      "On a Mac this is usually privacy protection. To include those folders, turn on Lumen in System Settings › Privacy & Security › Full Disk Access, then scan again.",
  },
  details: {
    title: "Scan details",
    entries: (entries: string) => `${count(entries)} entries read`,
    files: "Files",
    fileCount: (files: string) => count(files),
    size: "Size",
    sizeValue: (logical: string, allocated: string) => `${logical} (${allocated} on disk)`,
    reclaimable: "Can be freed",
    atLeast: (size: string) => `at least ${size}`,
    complete: "Everything was read.",
    incomplete: (denied: string, failed: string, notScanned: string) =>
      `Not everything could be read: ${count(denied)} denied, ${count(failed)} failed, ${count(notScanned)} not scanned.`,
  },
  confirm: {
    title: (n: number, size: string) => `Move ${items(n)} (${size}) to Quarantine?`,
    checked:
      "Each item is checked again first. Anything that has changed since the scan stays where it is.",
    restorable:
      "You can restore everything from Quarantine for at least 30 days, until you remove it for good.",
    nothingLeft: "There is nothing to move any more. Scan again to see what can be cleaned up.",
    move: "Move to Quarantine",
    moving: "Moving…",
    failed: "The clean-up could not run.",
    planMade: (createdAt: string) => `Plan made ${createdAt}, valid for 24 hours.`,
    planHash: "Plan hash",
  },
  done: {
    title: (n: number, size: string) => `Moved ${items(n)} (${size}) to Quarantine.`,
    noneMoved: "Nothing was moved.",
    freedNote: "Space is freed when you remove them for good.",
    openQuarantine: "Open Quarantine",
    notMoved: (n: number) => `${items(n)} not moved`,
  },
  item: {
    quarantined: "Moved to Quarantine (can be restored).",
    skipped: (why: string) => `Not moved: ${why}.`,
    failed: (reason: string, where: string) => `The move failed (${reason}); ${where}.`,
    skipChanged: "it changed or can no longer be checked",
    skipOutsideRoots: "it is outside the scanned folders",
    skipNowKeep: "Lumen now keeps it",
    skipNowReview: "Lumen now wants you to review it",
    skipReasonsChanged: "it would now be suggested for different reasons",
    skipNotPrepared: "it could not be recorded for a safe move",
    skipManifest: "its record could not be written",
    leftSource: "it is untouched in its place",
    leftQuarantine: "it is in Quarantine and can be restored",
    leftGone: "it is gone",
  },
  quarantine: {
    title: "Quarantine",
    count: (n: number) => `${items(n)} held`,
    total: (size: string) => `Remove them for good to free ${size}`,
    totalAtLeast: (size: string) => `Remove them for good to free at least ${size}`,
    intro:
      "Items you clean up wait here. Restore puts an item back where it was; removing it for good frees its space.",
    loadFailed: "Quarantine could not be loaded.",
    empty: "Quarantine is empty",
    emptyHint: "Items you clean up wait here, so you can restore them if you change your mind.",
    goCleanUp: "Go to Clean up",
  },
  held: {
    unknownPath: "(unknown location)",
    days: (days: number) =>
      days === 0
        ? "Moved here today"
        : `In Quarantine for ${count(days)} ${days === 1 ? "day" : "days"}`,
    afterFailure:
      "Kept here safely after a move went wrong. It can be restored, but not removed for good.",
    restore: "Restore",
    removeForGoodAsk: "Remove for good…",
    removeForGoodWarning:
      "This deletes it permanently and cannot be undone. Lumen first checks that nothing is using it.",
    removeForGood: "Remove for good",
    failed: "That did not work.",
  },
  restore: {
    restored: "Restored to its original place.",
    parentReplaced: "Its folder had been recreated since.",
    unverified: "It is back but did not match its record exactly; check it.",
    notRestored: (why: string) => `Not restored: ${why}. It stays in Quarantine.`,
    nameTaken: "something now has its name at the original place; move that away and try again",
    originUnreachable: "its original folder cannot be reached safely",
    itemChanged: "the item changed while in Quarantine",
    moveFailed: "the move back failed",
    notRestorable: "This item cannot be restored in its current state.",
  },
  finalize: {
    finalized: "Removed for good.",
    foundAtSource: "It is back at its original place already; nothing was removed.",
    notFound: "It is not in Quarantine; nothing was removed.",
    failed: (what: string) => `Could not remove it completely: ${what}.`,
    removedNothing: "nothing was removed",
    removedPartly: "part of it was removed; try again to finish",
    removedItem: "the item is gone, only its record remains",
    maybeInUse:
      "It may be in use, or that cannot be checked; nothing was removed. Close the programs that use it and try again.",
    notFinalizable: "This item can only be restored, not removed for good.",
  },
  settings: {
    title: "Settings",
    permissions: {
      title: "Permissions",
      fda: "Full Disk Access",
      fdaIntro:
        "macOS protects some folders, such as Mail, Messages and other apps' data. Lumen reads them only if you allow it.",
      steps: "To allow it:",
      step1: "Open System Settings.",
      step2: "Go to Privacy & Security › Full Disk Access.",
      step3: "Turn on Lumen, then scan again.",
      access: "Folder access",
      accessIntro:
        "Your computer may refuse Lumen some folders. Lumen then reads everything it is allowed to.",
      unread:
        "A folder Lumen cannot read is shown as unread, never as empty, and nothing in it is suggested for clean-up.",
    },
    about: {
      title: "About",
      version: "Version",
      local: "On this device",
      localBody:
        "Scans, decisions and Quarantine stay on this computer. File contents are never sent anywhere; AI review, if you turn it on, sends only metadata.",
    },
  },
  whatIs: (term: string) => `What does “${term}” mean?`,
  glossary: {
    scan: {
      title: "Scan",
      body: "Scanning reads your folders and measures what is inside them. It only reads: nothing is moved, changed or deleted. Lumen looks only at details such as names and sizes; it never opens your files and never downloads files that are stored only in the cloud.",
    },
    coverage: {
      title: "Unreadable folders",
      body: "Sometimes Lumen is not allowed to read a folder, or reading it fails. Lumen never treats such a folder as empty and never suggests removing anything it could not check, so it is simply left alone. Sizes that include it are lower bounds: the real figure may be larger.",
    },
    reclaimable: {
      title: "Space you can free",
      body: "How much disk space you get back once the items are removed for good. Space that other files still share (such as copies or snapshots) is not counted. Moving items to Quarantine does not free space yet; removing them for good does.",
    },
    verdicts: {
      title: "How Lumen decides",
      body: "Lumen decides about every item with fixed safety rules, not by guessing. “Can be cleaned up”: items such as caches that their app or tool re-creates. “Needs your review”: items Lumen is not sure about; they are never moved automatically. “Kept”: items Lumen will not touch, such as system files, files in use or files synced with the cloud. When in doubt, Lumen keeps the item.",
    },
    quarantine: {
      title: "Quarantine",
      body: "Quarantine is a safe holding place. Cleaning up moves items there instead of deleting them, and every item can be put back where it was. Items in Quarantine still use disk space until you remove them for good.",
    },
    finalize: {
      title: "Remove for good",
      body: "Removing for good deletes an item from Quarantine permanently and frees its space. It is the only step that cannot be undone. Lumen first checks that nothing is using the item, and removes nothing if it is in use or that cannot be checked.",
    },
    restore: {
      title: "Restore",
      body: "Restoring puts an item back in its original place, under its original name. Lumen never overwrites anything: if something else now has that name, or the original folder cannot be reached safely, the item stays in Quarantine and Lumen tells you why.",
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
