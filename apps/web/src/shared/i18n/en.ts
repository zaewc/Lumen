import type { ConfidenceView, ItemKindView } from "@lumen/api-client";
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
    overview: "Storage",
    cleanUp: "Explore",
    quarantine: "Quarantine",
    settings: "Settings",
    held: (n: number) => `${items(n)} in Quarantine`,
  },
  common: {
    loading: "Loading…",
    close: "Close",
    details: "Details",
    tryAgain: "Try again",
    scanDisk: "Scan disk",
    scanning: "Scanning…",
    scanAgain: "Scan again",
    items: (n: number) => items(n),
  },
  overview: {
    title: "Storage",
    disk: {
      title: "Startup disk",
      meter: "Disk usage",
      used: (size: string) => `${size} used`,
      available: (size: string) => `${size} available`,
      total: (size: string) => `${size} total`,
      percentUsed: (percent: string) => `${percent}% used`,
      low: "Almost full: less than 10% is available.",
      loadFailed: "Lumen could not read disk usage.",
    },
    session: {
      title: "Last scan",
      canFree: "Proposed for clean-up",
      command: "Clean up with its own command",
      review: "Needs review",
      quarantine: "In Quarantine",
      atLeast: (size: string) => `at least ${size}`,
      found: (n: string) => `${n} found`,
      showProgress: "Show progress",
      cleaned: (n: number, size: string) => `${items(n)} (${size}) moved to Quarantine.`,
      none: "No scan yet. Scan your disk to explore its contents and the evidence behind each decision.",
    },
  },
  workspace: {
    available: "Available space",
    assessed: "Items in this scan · by size",
    assessment: (n: number) => `${items(n)} assessed`,
    origin: "Origin",
    unknownOrigin: "No origin information",
    trail: "Origin and decision",
  },
  start: {
    title: "No scan yet",
    promise: "Scan the whole disk, or only some folders. Lumen asks before it moves anything.",
    wholeDiskHint:
      "A scan only reads, and takes a few minutes. Your computer may ask you to allow access to some folders.",
    chooseFolders: "Scan only some folders",
    foldersLabel: "Folders to scan, one per line",
    foldersEmpty: "Enter at least one folder.",
    scanFolders: "Scan these folders",
    failed: "The scan could not finish. Your files are unchanged.",
    aiOn: (service: string) =>
      `AI review is on: Lumen sends the details of proposed items and of some items to review, never their contents, to the ${service} server. You can turn it off in Settings.`,
  },
  ai: {
    title: "AI review",
    offIntro:
      "An AI model takes a second look at what Lumen proposes to clean up, and says what items to review likely are. It can only make Lumen more careful.",
    onIntro: (key: string) =>
      `On, with ${key}. Lumen sends the details of each proposed item and of up to 20 items to review, never their contents. It can only make Lumen more careful. You can turn it off at any time.`,
    keyInUse: {
      builtin: (service: string) => `${service} (built-in key)`,
      stored: (service: string) => `${service} (your key)`,
    },
    checking: "Checking…",
    short: {
      on: "AI review on",
      off: "AI review off",
    },
    loadFailed: "Lumen could not load the AI review setting.",
    changeFailed: "Lumen could not change the AI review setting.",
    consent: {
      title: "Turn on AI review?",
      what: "For each proposed item, and for up to 20 of the largest items to review, Lumen sends its path (with your home folder shown as ~), size, age and the evidence behind the decision. File contents are never sent.",
      whereJeb:
        "Lumen sends it with your API key to your school's Jeb server (jeb.https.gsmsv.site). The server's operator may keep the requests.",
      whereJebBuiltin:
        "Lumen sends it with its built-in key to a school's Jeb server (jeb.https.gsmsv.site). The server's operator may keep the requests.",
      whereOpenRouter: "Lumen sends it with your API key to TypeSafe's Jev through OpenRouter.",
      cautious:
        "The AI can only make a decision more cautious. For items to review, it only adds a note on what each likely is. It never chooses what is moved or removed.",
      revocable:
        "AI review then runs with every scan. You can turn it off in Settings at any time.",
      keyLabel: "Jeb API key",
      keyHint: "Lumen keeps it in your system's secure storage and never shows it again.",
      keyInvalid: "Paste the whole key: 1–256 characters, no spaces.",
      ownKey: "Use your own key",
      ownKeyHint:
        "Optional. Lumen uses it instead of the built-in key, keeps it in your system's secure storage and never shows it again.",
      confirm: "Turn on AI review",
      turningOn: "Turning on…",
    },
  },
  scanning: {
    title: "Scanning…",
    deciding: "Deciding…",
    decidingHint: "Lumen is applying its safety rules to what it found.",
    readOnly: "Lumen only reads and changes nothing.",
    progress: "Scan progress",
    items: "Items found",
    size: "Size on disk",
    unreadable: "Unreadable folders",
    reading: "Reading",
    failed: "Lumen could not decide about what it found. Your files are unchanged.",
  },
  results: {
    canFree: (size: string) => `You can clean up ${size}`,
    nothingTitle: "Nothing to clean up",
    nothing: "Lumen proposes only items it is sure are safe to set aside.",
    filter: "Show",
    filters: {
      all: "All",
      quarantine: "Proposed",
      command: "Clean up with its own command",
      review: "Needs review",
      keep: "Kept",
    },
    groups: {
      command: "Clean up with its own command",
      developer: "Developer tools",
      app_cache: "App caches",
      app_data: "App data",
      other: "Other",
    },
    otherTools: "Other tools",
    commandNote:
      "Only their tool should remove these. Open one to run its command; check what it removes first, as it cannot be undone.",
    status: {
      quarantine: "Proposed",
      command: "Own command",
      review: "Needs review",
      keep: "Kept",
    },
    columns: {
      name: "Name",
      location: "Location",
      status: "Status",
      size: "Size",
    },
    listHint: "Move between items with the up and down arrow keys.",
    emptyFilter: "No items with this status.",
    partial: "The scan stopped early. Here is what it found until then.",
    outOfSpace:
      "The scan stopped because the disk is almost full (Lumen always leaves 2 GB free). Here is what it found until then. Free some space and scan again.",
  },
  action: {
    cleanUp: "Clean up…",
    preparing: "Preparing…",
    nothing: "Nothing is proposed for clean-up.",
    planFailed: "Lumen could not prepare the clean-up. Your files are unchanged.",
  },
  detail: {
    size: "Size",
    location: "Location",
    decision: "Decision",
    reasons: "Evidence",
    aiTitle: "AI review advised caution.",
    aiBody: "So Lumen leaves it for you to review instead of proposing it.",
    aiNoteHint: "A guess from AI review. It does not change Lumen's decision.",
  },
  aiNote: {
    likely: (kind: string, confidence: string) => `AI: likely ${kind} (${confidence})`,
    unknown: "AI: could not tell what this is",
    kinds: {
      app_settings: "an app's settings or data",
      user_documents: "your documents",
      cache: "a cache",
      logs: "logs",
      downloads_or_installers: "downloads or installers",
      developer_tool_data: "a developer tool's data",
      backup_or_archive: "a backup or archive",
      unknown: "something it could not tell",
    } satisfies { readonly [K in ItemKindView]: string },
    confidence: {
      low: "low confidence",
      medium: "medium confidence",
      high: "high confidence",
    } satisfies { readonly [C in ConfidenceView]: string },
  },
  toolCommand: {
    title: (tool: string) => `${tool} can clean this up itself`,
    copy: "Copy",
    copied: "Copied",
    copyFailed: "Lumen could not copy it. Select the command and copy it yourself.",
    note: "Lumen does not run this command. Check what it removes before you run it.",
    run: "Run command",
    runConfirm: "Run",
    runWarning:
      "This command deletes files right away. They do not go through Quarantine, so Lumen cannot restore them. Check what it removes before you run it.",
    removesData:
      "It removes Docker's stopped containers, unused networks, dangling images and unused build cache.",
    running: "Running… this can take a few minutes.",
    succeeded: "The command finished. Scan again to see the space it freed.",
    failed: (code: number | null) =>
      code === null ? "The command failed." : `The command failed (exit code ${code}).`,
    timedOut: "It was still running after 15 minutes, so Lumen stopped it.",
    output: "Show output",
    runFailed: "Lumen could not run the command.",
    runNote: "Running it cannot be undone. Check what it removes before you run it.",
  },
  coverage: {
    incomplete: "Lumen could not read some folders and left them alone. Sizes may be larger.",
    howTo: "How to allow Full Disk Access",
  },
  details: {
    title: "Scan details",
    entries: (entries: string) => `${count(entries)} entries read`,
    files: "Files",
    fileCount: (files: string) => count(files),
    size: "Size",
    sizeValue: (logical: string, allocated: string) => `${logical} (${allocated} on disk)`,
    reclaimable: "Space you can free",
    atLeast: (size: string) => `at least ${size}`,
    complete: "Lumen read everything.",
    scanOther: "Scan other folders",
    incomplete: (denied: string, failed: string, notScanned: string) =>
      `Lumen could not read everything: ${count(denied)} denied, ${count(failed)} failed, ${count(notScanned)} not scanned.`,
  },
  confirm: {
    title: (n: number, size: string) => `Move ${items(n)} (${size}) to Quarantine?`,
    checked:
      "Lumen checks each item again first and leaves anything that changed since the scan where it is.",
    restorable:
      "You can restore everything from Quarantine for at least 30 days, until you remove it for good.",
    nothingLeft: "There is nothing left to move. Scan again to see what you can clean up.",
    move: "Move to Quarantine",
    moving: "Moving…",
    failed: "Lumen could not run the clean-up.",
    planMade: (createdAt: string) =>
      `Lumen made this plan ${createdAt}. You can use it for 24 hours.`,
    planHash: "Plan hash",
  },
  done: {
    title: (n: number, size: string) => `Moved ${items(n)} (${size}) to Quarantine.`,
    noneMoved: "Lumen did not move anything.",
    freedNote: "You get the space back when you remove them from Quarantine for good.",
    openQuarantine: "Open Quarantine",
    notMoved: (n: number) => `${items(n)} not moved`,
  },
  item: {
    quarantined: "Moved to Quarantine. You can restore it.",
    skipped: (why: string) => `Not moved: ${why}.`,
    failed: (reason: string, where: string) => `The move failed: ${reason}; ${where}.`,
    failIdentityMismatch: "it changed after the scan",
    failInUse: "a program is using it",
    failVolumeUnsupported: "this drive cannot hold a quarantine",
    failMoveFailed: "moving it went wrong",
    failVerificationFailed: "after the move it did not match the plan",
    failOther: "for a reason this version does not know",
    skipChanged: "it changed or can no longer be checked",
    skipOutsideRoots: "it is outside the scanned folders",
    skipNowKeep: "Lumen now keeps it",
    skipNowReview: "Lumen now wants you to review it",
    skipReasonsChanged: "it would now be suggested for different reasons",
    skipNotPrepared: "Lumen could not record it for a safe move",
    skipManifest: "Lumen could not write its record",
    leftSource: "it is untouched in its place",
    leftQuarantine: "it is in Quarantine and can be restored",
    leftGone: "it is gone",
  },
  quarantine: {
    title: "Quarantine",
    atLeast: (size: string) => `at least ${size}`,
    columns: {
      name: "Name",
      location: "Original location",
      size: "Size",
      held: "Held",
    },
    loadFailed: "Lumen could not load Quarantine.",
    empty: "Quarantine is empty",
    emptyHint: "Items you clean up wait here, so you can restore them if you change your mind.",
    goCleanUp: "Go to Clean up",
    selection: {
      all: "Select all",
      item: (name: string) => `Select ${name}`,
      count: (n: number, size: string) => `${items(n)} selected · ${size}`,
      clear: "Clear selection",
      restore: "Restore",
      removeAsk: "Remove for good…",
      removeWarning: (n: number) =>
        `This deletes ${items(n)} permanently, and you cannot undo it. Lumen first checks that nothing is using each one.`,
      removeSkips: (n: number) =>
        `${items(n)} that can only be restored ${n === 1 ? "stays" : "stay"} in Quarantine.`,
      remove: "Remove for good",
      working: (done: number, total: number) => `Working… ${count(done)} of ${count(total)}`,
      restored: (well: number, total: number) =>
        well === total
          ? `Restored ${items(total)}.`
          : `Restored ${count(well)} of ${items(total)}. The rest are still in Quarantine.`,
      removed: (well: number, total: number) =>
        well === total
          ? `Removed ${items(total)} for good.`
          : `Removed ${count(well)} of ${items(total)} for good. Lumen removed nothing else.`,
    },
  },
  held: {
    unknownPath: "(unknown location)",
    days: (days: number) =>
      days === 0 ? "Today" : `${count(days)} ${days === 1 ? "day" : "days"}`,
    restoreOnly: "Restore only",
    afterFailure: "Lumen kept this here safely after a move went wrong. You can only restore it.",
    restore: "Restore",
    removeForGoodAsk: "Remove for good…",
    removeForGoodWarning:
      "This deletes it permanently, and you cannot undo it. Lumen first checks that nothing is using it.",
    removeForGood: "Remove for good",
    failed: "Lumen could not do that. Try again.",
  },
  restore: {
    restored: "Restored to its original place.",
    parentReplaced: "Its folder had been recreated since.",
    unverified: "It does not exactly match its record, so check it.",
    notRestored: (why: string) => `Lumen could not restore it. It stays in Quarantine. ${why}.`,
    nameTaken: "Something now has its name at the original place. Move that away and try again",
    originUnreachable: "Lumen cannot safely reach its original folder",
    itemChanged: "It changed while in Quarantine",
    moveFailed: "Moving it back failed",
    notRestorable: "Lumen cannot restore this item in its current state.",
  },
  finalize: {
    finalized: "Removed for good.",
    foundAtSource: "It is already back at its original place. Lumen removed nothing.",
    notFound:
      "Lumen could not find it in Quarantine. If it was held on another drive, connect that drive and try again. Lumen removed nothing.",
    failed: (what: string) => `Lumen could not remove it completely. ${what}.`,
    removedNothing: "Nothing was removed",
    removedPartly: "Part of it is gone. Try again to finish",
    removedItem: "The item is gone; only its record remains",
    maybeInUse:
      "It may be in use, or Lumen cannot check that, so Lumen removed nothing. Close the apps that use it and try again.",
    notFinalizable: "You can only restore this item.",
  },
  settings: {
    title: "Settings",
    permissions: {
      title: "Permissions",
      fda: "Full Disk Access",
      fdaIntro:
        "macOS protects some folders, such as Mail, Messages and other apps' data. If you allow it, Lumen can read them too.",
      steps: "To allow it:",
      step1: "Open System Settings.",
      step2: "Go to Privacy & Security › Full Disk Access.",
      step3: "Turn on Lumen, then scan again.",
      open: "Open System Settings",
      openFailed: "Lumen could not open System Settings. Open it from the Apple menu instead.",
      access: "Folder access",
      accessIntro:
        "Your computer may block Lumen from some folders. Lumen still reads every folder it is allowed to.",
      unread:
        "Lumen shows a folder it cannot read as unread, never as empty, and proposes nothing in it.",
    },
    about: {
      title: "About",
      version: "Version",
      local: "On this device",
      localBody:
        "Scans, decisions and Quarantine stay on this computer. File contents are never sent anywhere. While AI review is on, it sends only details such as paths and sizes.",
      update: {
        title: "Updates",
        checking: "Checking for a new version…",
        upToDate: "Lumen is up to date.",
        available: (version: string) => `Version ${version} is available.`,
        packageOnly:
          "This copy was installed from a package: get the new version from zaewc.github.io/Lumen.",
        checkFailed: "Lumen could not check for a new version.",
        check: "Check now",
        install: "Update and restart",
        installing: "Updating…",
        installingNote:
          "Lumen downloads the new version, checks its signature, installs it and restarts.",
        installFailed: "The update did not install. This version is unchanged.",
        sidebar: (version: string) => `Version ${version} available`,
      },
    },
  },
  whatIs: (term: string) => `What does “${term}” mean?`,
  glossary: {
    verdicts: {
      title: "How Lumen decides",
      body: "Lumen decides about each item with fixed safety rules, not by guessing. “Proposed”: items such as caches that their app or tool re-creates. “Clean up with its own command”: items only their tool should remove; Lumen shows the tool's command for you to run. “Needs review”: large items Lumen is not sure about, or an uninstalled app's leftovers; they are never moved automatically. “Kept”: items Lumen will not touch, such as system files, files in use, apps' own data or small items it does not recognise. When in doubt, Lumen keeps the item.",
    },
    quarantine: {
      title: "Quarantine",
      body: "Quarantine is a safe holding place. Cleaning up moves items there instead of deleting them, and you can restore them where they were until you remove them for good. Items in Quarantine still use disk space until then.",
    },
  },
  reasons: {
    location: (name: string) => `Kept: it is in a protected location (${name}).`,
    rules: {
      "default.review": "Needs review: Lumen does not recognise this item.",
      "default.keep-small": "Kept: too small to be worth reviewing.",
      "protect.os-protected": "Kept: the operating system protects this item.",
      "protect.system-file": "Kept: Windows marks this item as a system file.",
      "protect.cloud-backed":
        "Kept: it is synced with a cloud service, so removing it here could remove it everywhere.",
      "protect.in-use": "Kept: a running app is using it.",
      "protect.in-use-partial":
        "Kept: no app Lumen can see uses it, but Lumen could not check every app.",
      "protect.in-use-unknown": "Kept: Lumen has not checked whether anything uses it.",
      "protect.not-observed": "Kept: the scan has not observed this item.",
      "protect.incomplete-scan":
        "Kept: Lumen could not read part of it, so its contents are unknown.",
      "protect.location-data": "Kept: Lumen could not check its list of protected locations.",
      "protect.location-context": "Kept: Lumen could not check its list of protected locations.",
      "eligible.quarantine-unsupported":
        "Needs review: this device cannot move it to Quarantine reversibly.",
      "eligible.frees-nothing": "Kept: removing it would free no space.",
      "eligible.object-kind": "Needs review: it is a link or special file, not data.",
      "eligible.in-use-partial":
        "No app Lumen can see uses it. Lumen could not check system apps, so it checks again just before moving it.",
      "eligible.in-use-partial-single-user":
        "No app Lumen can see uses it. You are this computer's only user, so what Lumen could not check is the system's. It checks again just before moving it.",
      "classify.devkb-regenerable": "Proposed: the tool that made it can make it again.",
      "classify.devkb-tool-action":
        "Needs review: only the tool's own clean-up command should remove it, and you cannot undo that.",
      "classify.devkb-not-regenerable": "Needs review: nothing can make it again.",
      "classify.devkb-self-managed": "Kept: the tool that made it cleans it up automatically.",
      "classify.devkb-unknown":
        "Needs review: Lumen could not load the tool information that matches it.",
      "classify.vendor-declared-cache": "Needs review: its maker says you can delete it.",
      "classify.app-cache": "Proposed: it is an app's cache, which the app makes again.",
      "classify.app-state":
        "Kept: it holds an app's own data, settings or documents, so Lumen leaves it alone.",
      "classify.orphan": "Needs review: the app it belongs to seems to be uninstalled.",
      "jev.demote": "Needs review: AI review advised caution.",
    } satisfies { readonly [R in RuleId]: string },
  },
};

/** What every catalog provides: the same keys and message signatures as `en`. */
export type Messages = typeof en;

/** A concept the glossary explains. */
export type GlossaryTerm = keyof Messages["glossary"];
