# Lumen threat model

> Status: baseline, 2026-10-06. Method: STRIDE per data-flow element, LINDDUN for
> privacy, and an attack tree for the top risk ("unintended deletion"). Update this
> document in any PR that touches a CODEOWNERS-protected security path (policy,
> executor, IPC, workflows, Jev).

## 1. Scope and assumptions

In scope:

- the desktop app (Tauri host + core);
- the opt-in privileged helper (macOS, ADR-0063) and the future agent;
- the opt-in browser dashboard;
- mobile apps;
- the Jev providers;
- the build and release pipeline.

Assumptions:

- The operating system kernel and its security mechanisms (SIP, TCC, UAC, sandboxing)
  are not compromised.
- An attacker may control **file and directory names, file contents, metadata (xattrs,
  plists, registry values, package labels), and timing**. That includes other local
  processes running as the same user (malware, malicious repositories, compromised
  developer tools) and web pages open in the user's browser.
- A same-user attacker with arbitrary code execution can already delete the user's
  files. Lumen must not give such an attacker **more** power (privilege, persistence,
  or deletion of files the attacker cannot reach) and must not be the instrument of
  loss for unprivileged attackers who only control names or timing.

## 2. Assets

| Asset | Why it matters |
| --- | --- |
| User data (documents, media, credentials, keys, app state) | Irreplaceable; the primary thing Lumen must not destroy |
| Quarantine store and ledger | Required for rollback; tampering defeats reversibility |
| Evidence, decisions and Jev traces | Audit trail; integrity needed for explanation and investigation |
| Metadata about the device (paths, app lists, hashes) | Privacy-sensitive |
| Privileged helper (macOS, opt-in; ADR-0063) | Privilege escalation target |
| Signing keys, release pipeline | Supply-chain compromise of all users |
| Cloud Jev credentials (if any) | Abuse and cost |

## 3. Data-flow diagram and trust boundaries

```text
 [Untrusted FS content & metadata] ──(TB1: OS APIs)──▶ (Scanner adapters)
                                                          │
                                                          ▼
 (Webview UI) ──(TB2: Tauri IPC)──▶ (Core: evidence → graph → policy → planner) ──▶ [index.db]
 (Browser)   ──(TB3: loopback HTTP)──▶      │                    │                  [ledger.db]
                                            │                    ▼
                                            │            (Executor: lumen-fs-exec) ──▶ [Quarantine stores]
                                            │                    │
                                            │       (TB4: XPC / pipe, future)
                                            │                    ▼
                                            │            (Privileged helper)
                                            ▼
                               (TB5: network, user's choice) ──▶ (Cloud Jev provider)
 (Developer tools) ◀──(TB6: subprocess)── (Tool runner)
 (CI / release) ──(TB7: supply chain)──▶ (Signed artifacts) ──▶ users
```

## 4. STRIDE analysis

### TB1: Filesystem and OS metadata → scanner

| Threat | Example | Controls |
| --- | --- | --- |
| Tampering | Symlink or junction swapped in after scan (TOCTOU), making cleanup hit another target | Identity-keyed decisions; executor re-verifies `(dev, ino)` / `(vol, FileId)` by handle; no-follow, no-replace ops; race tests ([ADR-0016](../decisions/0016-single-handle-relative-executor.md)) |
| Tampering | Hard link to a sensitive file planted in a cache directory | Link count checked; `nlink > 1` items are "frees nothing" and `REVIEW` |
| Spoofing | Directory named like a known cache (`Caches/com.apple.Safari`) to inherit a safe classification | Classification requires corroborating evidence (owner app, signing team, vendor declaration), not names; unknown → `REVIEW` |
| Information disclosure, denial of service | An app keeps an embedded browser profile (cookies, saved sign-ins, site storage) in its `Caches` folder, which the app-cache classification would quarantine and later remove | Profile state recognised by entry names below the folder (`BrowserProfile`); `classify.browser-profile` keeps it. A planted marker name only makes Lumen keep more |
| Denial of service | Enormous or deep trees, sparse giant files, directory loops, slow network mounts | Bounded queues, per-volume semaphores, stall watchdog, no traversal of reparse points or mounts, timeouts; partial failure keeps the scan alive |
| Denial of service | Archive or decompression bombs | Archives are never opened |
| Information disclosure | Scanner reads file contents | Scanning remains metadata-only. After a non-partial scan, the separate duplicate comparison reads contents in the latest completed scan's scope (ADR-0065); it never reads placeholders |
| Information disclosure, denial of service | A large folder offered for review, or a known cache, holds `~/.ssh`, the keychains, a cloud sync folder or a browser profile (an app's folder in Application Support, a backup) that would be moved with it | A folder above a protected location is kept (`protect.contains-*`, by path); a directory with a browser profile's entry names anywhere below it gets `BrowserProfile` and is kept. A planted marker name only makes Lumen keep more |
| Tampering, denial of service | A materialised folder inside a sync root (a `node_modules` in a Dropbox project) is quarantined: moving it out of the sync root deletes it in the cloud and on every device | Entries at or below a placeholder, dataless or sync-root directory, and directories holding a placeholder or dataless item, are `CloudBacked` and kept (`protect.cloud-backed`); computed from scan flags only |
| Tampering | Cloud placeholder hydration triggered by scan | Placeholder detection from metadata; process-wide no-materialise policy on macOS; placeholder compatibility mode on Windows |
| Tampering | Malicious names (control characters, RTL override, newlines, invalid UTF-8, NFC/NFD and case twins) mislead the UI or the user | Bytes stored raw; display escaping with visible control characters; identity-based comparisons |

### TB2 and TB3: UI → core

| Threat | Example | Controls |
| --- | --- | --- |
| Elevation of privilege | Compromised webview (XSS, webview 0-day) issues arbitrary deletes | No path-accepting mutation commands: mutations name a plan ID or an operation ID; per-window capabilities; no `fs`/`shell` plugins; isolation pattern; strict CSP ([ADR-0004](../decisions/0004-desktop-shell-tauri.md), [ADR-0023](../decisions/0023-ipc-security.md)). **Host-rendered confirmation ([FS-1](public-beta-readiness.md#fs-1-webview-only-confirmation-can-authorize-irreversible-actions)):** `execute`, `uninstall`, `finalize` and `run_tool_command` first show a native dialog whose content the host builds from what it stored (the plan's items, each item the person chose listed by path, the held item's original path, the knowledge base's command and its targets); only its action button lets the command run, so a compromised webview holding a plan's hash can no longer move or remove anything without the person. The page sets only the language of the dialog's fixed sentences |
| Information disclosure | Compromised webview makes Lumen read file contents outside the intended scan scope | `compare_scan` takes a scan ID, not paths; the host requires it to match the latest completed scan and reads roots from the index. The comparison skips protected locations and quarantine stores, verifies files while reading, is bounded and cancellable, and returns paths, sizes and counts, never content digests ([ADR-0065](../decisions/0065-compare-completed-scan-roots-for-identical-copies.md)). Residual risk: `scan` accepts roots from the webview, so a compromised page can request a scan of a scope and then trigger content comparison in that latest scope; there is no second confirmation |
| Tampering | Compromised webview uses reassessment (ADR-0058) to make an item proposable | `reassess` takes item IDs from the latest assessment, never paths. It only reads (live in-use probe, re-listing, inventories); the unchanged policy decides, with no AI; protection precedence holds; an item that changed since the scan gets no verdict. Planning assesses again from the scan, and execution re-checks every item |
| Tampering, elevation of privilege | Compromised webview uninstalls an app the person did not choose, or names a path to move ([ADR-0062](../decisions/0062-uninstall-macos-apps-into-quarantine.md)) | `uninstall_preview` takes a software-list key (at most 4 KiB), matched only against the bundle inventory's identifiers, never used as a path; it only reads and stores a preview. `uninstall` takes the preview ID and its full hash, nothing else; the core refuses cleanup plans, re-checks ADR-0062's gate right before the move and refuses an app changed since the preview. The move is reversible: the app is held in Quarantine, chosen by the person, never removed automatically. As for cleanup, a compromised webview can read the hash it confirms ([FS-1](public-beta-readiness.md#fs-1-webview-only-confirmation-can-authorize-irreversible-actions)), so it could uninstall into quarantine any app the gate lets through |
| Tampering, denial of service | Compromised webview quits apps the person is using, or another account's processes (roadmap 16.3a) | `quit_app` takes a software-list key (at most 4 KiB), matched only against the bundle inventory's identifiers; never a path or process ID. It sends only `-[NSRunningApplication terminate]`, the request Quit in the app's menu sends, so the app can ask to save or decline; Lumen never force-quits or signals a process. Only instances whose bundle is that inventory bundle (same device and inode) and that run as the current account are asked, never Lumen itself, and a privileged Lumen refuses. A compromised webview could still ask each listed app to quit, which loses nothing an app's own Quit would not |
| Elevation of privilege | Compromised webview installs Lumen's privileged helper without the person, or uses it ([ADR-0063](../decisions/0063-use-administrator-rights-through-an-opt-in-helper.md)) | `helper_enable`, `helper_disable`, `helper_state` and `helper_open_login_items` take no arguments. Enabling only registers the bundled daemon with `SMAppService`; macOS does not start it until an administrator allows it in System Settings › Login Items with their password, outside the webview. An unsigned or privileged Lumen refuses. The helper answers only `status` so far (TB4) |
| Information disclosure, denial of service | Compromised webview uses app icons (roadmap 16.4) to read an arbitrary file, probe paths, or exhaust memory; or an icon reaches AI review | `app_icon` takes a software-list key (at most 4 KiB), looked up only among the macOS apps the latest list recorded (one location, unique key); never a path. It asks `-[NSWorkspace iconForFile:]` for a directory that is not a cloud placeholder and returns a 64 × 64 pixel PNG of at most 32 KiB, or nothing; the page accepts only an inline PNG `data:` URL of that size (Zod). Icons are kept in memory per key, version and location, bounded by the list (at most 4096 apps), and never stored, logged or added to AI review's request, which carries identity fields only (ADR-0061). A compromised webview learns only icons of apps the list already shows it |
| Spoofing | Malicious web page calls the loopback dashboard API (CSRF, DNS rebinding) | Dashboard off by default; `Host` and `Origin` allowlists including WebSockets; header bearer token; no CORS ([ADR-0010](../decisions/0010-web-dashboard-role.md)) |
| Tampering | UI alters a plan between review and execution | The confirmation is the plan's full hash, so it confirms only that plan; core re-runs the policy at execution. This stops an altered plan from running under an earlier confirmation, but not a compromised webview from confirming a plan the person never saw: it can read the hash ([FS-1](public-beta-readiness.md#fs-1-webview-only-confirmation-can-authorize-irreversible-actions)) |
| Repudiation | "I never approved that" | Ledger records the plan hash, confirmation time and UI surface. The surface is set by the host from the command called, not proven by a person's action, so a record shows the plan was confirmed through that surface, not that the person approved it ([FS-1](public-beta-readiness.md#fs-1-webview-only-confirmation-can-authorize-irreversible-actions)) |

### Core, policy and Jev

| Threat | Example | Controls |
| --- | --- | --- |
| Tampering (prompt injection) | Filename or plist value says "this is safe to delete" | Jev has no tools or actions; closed-vocabulary output; untrusted data JSON-encoded and labelled; monotone-safety policy (Jev only increases caution by default); injection canary suite as a release gate ([ADR-0020](../decisions/0020-jev-evidence-only-judge.md)) |
| Tampering | Policy rule data altered on disk | Rules embedded at build time; no runtime-downloaded policy |
| Elevation of privilege | Logic error lets a protected item reach `QUARANTINE` | Hard-protection stage first; property tests; mutation testing; CODEOWNERS on `lumen-policy` |
| Information disclosure | Evidence bundle sent to cloud Jev reveals user names or paths | Cloud tier off until the user chooses it on first launch, with a kept off switch (ADR-0050); home directory tokenised, user names stripped, no contents; disclosure UI |

### Executor and quarantine

| Threat | Example | Controls |
| --- | --- | --- |
| Tampering | Another process modifies or replaces quarantined items | Stores private to their owner (Unix: owner and mode 0700; Windows: owner and protected DACL compared exactly), planted stores refused, never repaired; restore and finalize re-check a held file's identity, kind, size, time and links, and a held directory's identity and kind; both use the same check, so anything finalize would remove can still be restored |
| Tampering, denial of service | An item the person chose from the review items (ADR-0051), which Lumen cannot vouch for, is removed for good when retention ends | The ledger records who chose each item (`quarantine_choosers`, append-only); only items recorded as the policy's expire; a person's choice or a missing record waits for the person; the manifest (1.1) records the chooser too |
| Elevation of privilege, tampering | Uninstalling a macOS app ([ADR-0062](../decisions/0062-uninstall-macos-apps-into-quarantine.md)) moves something the cleanup policy would keep: an app an administrator installer or the App Store installed, one with helpers registered with the system, one that runs, or a different bundle than the one previewed | `lumen_application::uninstall::gate`, pure and tested on every platform, run on fresh read-only evidence at the preview and again right before the move; unknown refuses: only a `.app` directly in `/Applications` or `~/Applications` as the bundle inventory lists it, never under `/System` or `/Library/Apple`, bundle directory owned by the running user (root-owned `.pkg` and App Store installs refused), no linked background item (any method, plus a program inside the bundle ignoring letter case, a program link resolving into it, or a label starting with the bundle ID ignoring case; an item running `/usr/bin/open`, or an incompletely read background-item inventory, refuses), none of `Contents/Library/{LoginItems, LaunchServices, LaunchAgents, LaunchDaemons, SystemExtensions}` (metadata only, no link followed), not running, a store on its volume. The preview is a one-item plan chosen by the person and confirmed by its full hash, at most 24 hours old and used once; its decision cites `uninstall.macos-app`, which the policy never fires, so cleanup execution refuses an uninstall plan and an uninstall refuses a cleanup plan. The decision's evidence hash binds the bundle ID, name, version, path, identity and size, so an app changed since its preview is refused. The move is the journaled, no-replace executor path, after an identity check that follows the executor's own record, with the in-use re-probe (a partial answer, where only other users' open files are unseen, is accepted only because the gate just found no process of any user running from the bundle, by executable path; amended ADR-0062); privileged processes are refused; the ledger records the person as chooser, so it is never removed automatically |
| Denial of service | Crash mid-move loses track of a file | Durable journal intent before each move; startup reconciliation resolves every unfinished operation from where the item is, and leaves contradictions for the user (roadmap 9.6c), when the desktop app starts and before held items are listed (UX-3); a ledger lock keeps two processes from reconciling at once or over a move under way; crash points tested at every journal step (9.9c) |
| Tampering | Restore overwrites a newer file at the original path | No-replace rename in both directions; a taken name is a conflict for the user |
| Elevation of privilege | Finalize follows a link or a mount point inside a quarantined directory and removes something else | The whole tree is checked first (same volume, depth limit, readable); links and junctions are removed as themselves; entries opened by handle relative to their directory |
| Repudiation / integrity | An executor error misreports where it left an item, so the journal records the wrong location | Every error reports `Left`/`Removed`; fault points after each system call test that the report matches the filesystem ([ADR-0032](../decisions/0032-test-only-fault-points.md)) |
| Elevation of privilege | Memory-safety bug in Windows FFI | `unsafe` confined to `win_sys`, each block with a SAFETY comment; CODEOWNERS and the [executor review checklist](executor-review.md) ([ADR-0033](../decisions/0033-windows-executor-mechanics.md)) |
| Information disclosure | Quarantine store indexed or backed up to the cloud | macOS: sticky Time Machine exclusion of each store; Windows: stores not content-indexed. macOS has no documented programmatic Spotlight exclusion, so stores stay indexed there (owner-only) |

### TB4: Core → privileged helper (macOS, ADR-0063)

`lumen-helper` is a launch daemon the app registers with `SMAppService`, started by
launchd on demand and exiting after a minute idle. As of roadmap 16.3b it answers
`status` only: it moves, deletes and opens nothing. The rows below are its controls
now; each later verb (16.3c–d) adds its own.

| Threat | Example | Controls |
| --- | --- | --- |
| Spoofing (client) | Another program, or another account's process, sends the helper requests | One XPC Mach service in the system domain. The listener and every accepted peer require a Developer ID signature by the helper's own team with Lumen's identifier `dev.lumen.desktop` (`xpc_connection_set_peer_code_signing_requirement`, checked by XPC on every message's audit token, so a reused PID cannot pass); requests failing it are dropped. The team is read from the helper's own valid signature: an unsigned or ad hoc build has none and refuses to listen. Requirement text is built only from ASCII letters, digits, `.` and `-` |
| Spoofing (helper) | A program poses as the helper to read Lumen's requests or forge answers | The client looks the service up only in the system domain (`XPC_CONNECTION_MACH_SERVICE_PRIVILEGED`), where only launchd jobs registered by an administrator live, and requires the helper's identifier `dev.lumen.desktop.helper` and the same team on every reply |
| Elevation of privilege | Confused deputy: the request names another user, or a path | The caller's user comes from the connection (`xpc_connection_get_euid`), never from the request or the helper's environment; a field naming a user is ignored. A root caller is refused. Requests are parsed into a closed set of verbs, anything else is refused (`lumen_application::helper::answer`, pure and tested) |
| Denial of service | Large or endless requests keep a root process busy | Bodies over 64 KiB are refused at the transport; each request gets one reply; the daemon exits when idle and launchd starts it again on demand |
| Tampering | The helper binary is replaced inside the app bundle, or its identifier changes in an update | The bundle is sealed by the app's Developer ID signature and notarized; launchd ties the registered daemon to its signature's identifier, which is embedded in the helper's Info.plist section and never changes (research 37) |
| Tampering | Version skew between app and helper | `status` reports the protocol version; a client that sees another one sets the helper up again |

### TB5: Cloud Jev

| Threat | Example | Controls |
| --- | --- | --- |
| Information disclosure | Provider retains metadata | Metadata only, never file contents; home folder redaction; nothing sent until the user chooses, asked once on first launch and kept ([ADR-0050](../decisions/0050-ask-about-ai-review-on-first-launch.md)); zero-retention endpoints on OpenRouter; Jeb publishes no retention policy, and the disclosure says its operator may keep requests ([ADR-0036](../decisions/0036-jeb-on-the-school-ai-hub.md)) |
| Information disclosure | While AI review is on, the names, publishers and versions of installed software reach the chosen AI service, describing a person's tools, interests and sometimes employer ([ADR-0061](../decisions/0061-send-installed-software-to-ai-review.md)) | Sent only through `explain_software`, only while AI review is on, and checked again before every request; identity fields only (origin, identifier, name, publisher, version, protected and hidden flags, up to 8 linked background items), never a path, user name, usage signal or size; each item at most once per origin, identifier, version, model and prompt, with the notes cached on the device (`software-notes.json`, at most 2000 entries, no paths); at most 40 per call; every request traced in the ledger; the list stays out of logs; the ADR-0050 off switch stops it |
| Tampering | Response from an unexpected model or a refusal parsed as data | Pinned model ID check, `stop_reason` handling, schema validation, unknown evidence IDs rejected |
| Spoofing | API key theft from local storage | Keys stored in the OS keychain or credential manager; never logged |
| Information disclosure | Lumen's built-in Jeb key read out of a release binary and its quota spent elsewhere | **Accepted risk** ([ADR-0042](../decisions/0042-ship-a-built-in-jeb-key-in-release-builds.md)): not obfuscated, as that would not protect it; compiled in only on release builds from `main` through the `release` environment, never in pull request runs; the AI Hub's operator approved this use (owner confirmation, 2026-10-30); a stored user key wins; revocable, and a failed judge only means no judgment; rotation is a new release |

### TB6: Developer tool commands

| Threat | Example | Controls |
| --- | --- | --- |
| Elevation of privilege | A tool command runs with administrator rights | The desktop host refuses privileged execution; programs are resolved to absolute executable paths from the user's search directories, as in their terminal (ADR-0049) |
| Tampering | The page supplies an arbitrary command | The page names a knowledge-base entry matched by the latest assessment; the host looks up its reviewed program and literal arguments; no shell, one command at a time, explicit confirmation of irreversible removal, shown by the page; the host takes no confirmation ([FS-1](public-beta-readiness.md#fs-1-webview-only-confirmation-can-authorize-irreversible-actions)) |
| Tampering | Tool output misparsed into a false success | Success comes from the exit status, never output parsing; output is bounded display text; a new scan measures the resulting space |
| Tampering, information disclosure | Tool storage inspection (`tool_storage`) is used to run something, or a hostile tool's output spoofs the page | The command takes no input: fixed read-only queries (`xcrun simctl runtime list --json`, `docker … system df`, `brew --cache`, `npm config get cache`, `pnpm store path`), no shell, closed input, 30 s and 8 KiB bounds, Docker pinned to the context it reports; output is validated (complete rows, single-line absolute paths) and escaped as untrusted text, and a failure is shown as unknown, never as empty |

### TB7: Supply chain and CI

| Threat | Example | Controls |
| --- | --- | --- |
| Tampering | Compromised dependency (axios 2026-03, arrayref 2026-08) | Committed lockfiles, `--locked`, cargo-deny, OSV-Scanner, pnpm release-age gating, review of new packages, cargo-vet for sensitive crates |
| Tampering | Compromised action or tag force-push (Trivy 2026-03) | Actions pinned by SHA, zizmor, actionlint, harden-runner, no `pull_request_target` |
| Elevation of privilege | PR job reads secrets or poisons caches | Minimal permissions; caches saved only on `develop`; secrets only in the protected release environment |
| Denial of service, tampering | A runner loses communication before reporting the documentation classification, or an uncertain classification skips code checks | Classifier jobs retain blocking egress with GitHub's required runner/service/storage endpoints allowed and a five-minute timeout; only an explicit `code=false` skips expensive checks; documentation link checks and the individual required matrix check names remain |
| Tampering | A token or account with write access merges into `main` and publishes an update every installed copy accepts | **Accepted risk** ([ADR-0054](../decisions/0054-release-on-every-push-to-main.md)): no person approves a release; pull requests and required checks on `main` are the gate; the release job publishes only after checksums and the updater manifest verify; the signing key never reaches a build |
| Spoofing | Users install a tampered build | Code signing, notarization, artifact attestations, immutable releases |
| Tampering, loss of work | A coding-agent session dies before handing off, a second writer races it, or stale test/PR evidence is reused | The [local recovery supervisor](../operations/agent-recovery.md) checkpoints actual Git state, retains an inherited per-worktree lock through process cleanup, verifies current local fingerprints and GitHub head/target SHAs, and gates remote writes on task authorization; trusted local developer tooling, not a sandbox against same-user malicious agents |

## 5. Privacy (LINDDUN)

| Category | Concern | Control |
| --- | --- | --- |
| Linking / identifying | Content hashes and app lists fingerprint a user | Hashes and inventories never leave the device by default |
| Detecting | Telemetry reveals the presence of sensitive apps | No paths, names or app lists in metrics; telemetry opt-in |
| Disclosure | Exported diagnostics contain paths | User reviews the export; redaction layer on by default |
| Unawareness | Users don't know what cloud Jev sends | The first-launch question says what is sent and where; disclosure in Settings and at scan start; the choice is kept per user (ADR-0050) |
| Non-compliance | Store privacy declarations wrong | Privacy manifest and Play data-safety form generated from `docs/operations/privacy.md` (planned) |

## 6. Attack tree: unintended deletion of user data

```text
GOAL: a file the user needs is permanently lost because of Lumen
├── 1. Wrong item selected
│   ├── 1.1 Misclassification (name spoofing, poisoned metadata)      → corroborating evidence; unknown → REVIEW
│   ├── 1.2 Blind spot read as absence (TCC-denied, truncated lists)  → AccessState + CoverageReport
│   ├── 1.3 AI injection promotes the item                             → monotone safety; canary gate
│   └── 1.4 Policy bug                                                 → property + mutation tests; hard-protection stage
├── 2. Right item, wrong target at execution
│   ├── 2.1 Symlink/junction swap (TOCTOU)                             → handle-relative executor, identity re-check
│   ├── 2.2 Hard link or clone shares data with a needed file          → link-count/clone checks; "frees nothing"
│   └── 2.3 Path normalisation or case collision                       → identity, not string, comparisons
├── 3. Reversibility defeated
│   ├── 3.1 Crash mid-move                                             → journal + reconciliation
│   ├── 3.2 Ledger lost                                                → sidecar manifests
│   ├── 3.3 Cross-volume copy-and-delete                               → refused; item becomes REVIEW
│   ├── 3.4 Retention expires unnoticed                                → explicit retention UI; finalization logged
│   └── 3.5 Irreversible action mistaken for reversible                → labelled actions; never batched together
└── 4. Side effects
    ├── 4.1 Cloud sync propagates a deletion                           → sync roots, their contents and placeholder holders KEEP; iCloud Photos warning
    └── 4.2 Tool command removes more than expected                    → dry run captured as evidence; user confirmation
```

## 7. Residual risks

- A same-user attacker with code execution can tamper with Lumen's data directory,
  including the quarantine store. This is mitigated by owner-only permissions and
  integrity data in manifests, but not prevented.
- Platform behaviour that is undocumented and drifts: macOS 27 AppData protection
  lists, the `StartupApproved` format, BTM versions, Recycle Bin internals.
- Statistical limits of Jev calibration for rare artifact classes. Those classes stay
  "Jev cannot promote".
- macOS quarantine stores remain visible to Spotlight for their owner: Apple documents
  no programmatic exclusion for a directory (research report 10, row 29).
- Windows cannot flush individual directory entries (`FlushFileBuffers` needs
  `GENERIC_WRITE`, not grantable on directories); a rename NTFS has not logged at a
  crash is resolved by reconciliation, never guessed.
- `parent_replaced` on restore is best effort: a recreated parent may reuse the old
  inode or file ID.
- A directory's contents are compared by its modification time, which advances in
  clock ticks (NTFS: up to ~15.6 ms; Linux: the coarse kernel clock). An entry added
  within the same tick as the directory's previous change, between preparation and
  the move, goes undetected and is quarantined with the directory; it stays
  restorable. Found by the Windows race suite on 2026-10-15.
- A held directory is checked by identity and kind only, on restore as on finalize, so
  entries added or removed while it is held (Finder's `.DS_Store`, an app still
  writing) neither block restoring it nor stop its removal. A directory whose
  finalization stopped part way can be restored with only what remains. Either way the
  restore is reported as not matching its record, so the user is asked to check it.
- A device number (`st_dev`) names a volume only while it stays mounted: macOS and
  Linux assign it at mount time, so after a restart or an update no store may be on a
  record's device, and its items would be stuck (2026-11-07). The Unix executor then
  opens the store on the base's current device, with its usual privacy checks, and
  accepts the item held there under its operation's name only if `check_held` matches
  it with the device number changed. A held directory is thereby re-identified by its
  name, inode and kind in the private store; every identity in the record is
  renumbered together, since a quarantined item never leaves its volume, and the
  original parent's identity is still checked on restore. Windows records volume
  serial numbers, which do not change on remount.
- Cloud sync folders are recognised from the scan's placeholder and sync-root flags
  and from the providers' documented default folders. A folder synced only through
  iCloud Desktop & Documents, or a Windows sync root moved away from its default,
  whose items carry no flag, can still be proposed
  ([research 32](../research/32-cloud-sync-folders.md)). Quarantine keeps it
  restorable, but the provider may already have deleted the cloud copy.
- An app uninstall's gate (ADR-0062) checks the owner and the helper folders by path
  metadata, not by handle, and the executor re-verifies only the bundle directory
  itself (identity, kind, size, time, links). A helper folder added inside the bundle
  between the gate and the move goes with it into quarantine; the move stays
  restorable. Unprivileged, the process table cannot show other users' open files.
  The running check therefore reads every process's executable path instead, and
  the in-use re-probe's partial answer is accepted after it. A system service
  holding a bundle file open keeps a valid descriptor across the rename, and the
  app can be restored (ADR-0062, amended 2026-11-08). A process whose executable was
  deleted after it started counts as running from no folder. Login items kept by Background Task
  Management cannot be read unprivileged, and a background item that starts the app
  through a script is not attributed to it; either can be left broken by the move,
  and restoring the app fixes it.
- A compromised webview can still ask for a clean-up, an uninstall, a removal for good
  or a tool command at any time, and a person who clicks through the host's own dialog
  without reading it authorizes it. The dialog names what it acts on, so this needs
  the person's mistake, not only the webview's
  ([FS-1](public-beta-readiness.md#fs-1-webview-only-confirmation-can-authorize-irreversible-actions),
  resolved by the host-rendered confirmation).

## 8. Security testing obligations

- Race tests (symlink/junction flips mid-execution) on macOS, Windows and Linux CI
  (`crates/lumen-fs-exec/tests/{races,names,windows}.rs`), fault points after every
  executor system call, and crash points at every journal step.
- Every executor change reviewed against the [executor checklist](executor-review.md).
- A malicious-name corpus (control, bidi, newline, invalid UTF-8, NFC/NFD and case
  twins, long paths).
- IPC peer-rejection tests per transport.
- An injection canary corpus for Jev, in both directions.
- Fuzzing of every parser of untrusted data (plist, registry exports, package
  manifests, `getattrlistbulk` buffers).
- Supply-chain checks as required CI gates
  ([ADR-0026](../decisions/0026-ci-and-supply-chain-security.md)).
