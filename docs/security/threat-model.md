# Lumen threat model

> Status: baseline, 2026-10-06. Method: STRIDE per data-flow element, LINDDUN for
> privacy, and an attack tree for the top risk ("unintended deletion"). Update this
> document in any PR that touches a CODEOWNERS-protected security path (policy,
> executor, IPC, workflows, Jev).

## 1. Scope and assumptions

In scope:

- the desktop app (Tauri host + core);
- the future agent and privileged helper;
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
| Privileged helper (future) | Privilege escalation target |
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
                               (TB5: network, opt-in) ──▶ (Cloud Jev provider)
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
| Denial of service | Enormous or deep trees, sparse giant files, directory loops, slow network mounts | Bounded queues, per-volume semaphores, stall watchdog, no traversal of reparse points or mounts, timeouts; partial failure keeps the scan alive |
| Denial of service | Archive or decompression bombs | Archives are never opened |
| Information disclosure | Scanner reads file contents | Metadata only; contents read only for duplicate hashing of user-selected scopes, never for placeholders |
| Tampering | Cloud placeholder hydration triggered by scan | Placeholder detection from metadata; process-wide no-materialise policy on macOS; placeholder compatibility mode on Windows |
| Tampering | Malicious names (control characters, RTL override, newlines, invalid UTF-8, NFC/NFD and case twins) mislead the UI or the user | Bytes stored raw; display escaping with visible control characters; identity-based comparisons |

### TB2 and TB3: UI → core

| Threat | Example | Controls |
| --- | --- | --- |
| Elevation of privilege | Compromised webview (XSS, webview 0-day) issues arbitrary deletes | No path-accepting mutation commands; plan IDs + confirmation tokens; per-window capabilities; no `fs`/`shell` plugins; isolation pattern; strict CSP ([ADR-0004](../decisions/0004-desktop-shell-tauri.md), [ADR-0023](../decisions/0023-ipc-security.md)) |
| Spoofing | Malicious web page calls the loopback dashboard API (CSRF, DNS rebinding) | Dashboard off by default; `Host` and `Origin` allowlists including WebSockets; header bearer token; no CORS ([ADR-0010](../decisions/0010-web-dashboard-role.md)) |
| Tampering | UI alters a plan between review and execution | Plan hash bound to the confirmation token; core re-runs the policy at execution |
| Repudiation | "I never approved that" | Ledger records the plan hash, confirmation time and UI surface |

### Core, policy and Jev

| Threat | Example | Controls |
| --- | --- | --- |
| Tampering (prompt injection) | Filename or plist value says "this is safe to delete" | Jev has no tools or actions; closed-vocabulary output; untrusted data JSON-encoded and labelled; monotone-safety policy (Jev only increases caution by default); injection canary suite as a release gate ([ADR-0020](../decisions/0020-jev-evidence-only-judge.md)) |
| Tampering | Policy rule data altered on disk | Rules embedded at build time; no runtime-downloaded policy |
| Elevation of privilege | Logic error lets a protected item reach `QUARANTINE` | Hard-protection stage first; property tests; mutation testing; CODEOWNERS on `lumen-policy` |
| Information disclosure | Evidence bundle sent to cloud Jev reveals user names or paths | Cloud tier opt-in; home directory tokenised, user names stripped, no contents; disclosure UI |

### Executor and quarantine

| Threat | Example | Controls |
| --- | --- | --- |
| Tampering | Another process modifies or replaces quarantined items | Owner-only permissions on stores; manifests record identity and size facts; restore verifies before rename |
| Denial of service | Crash mid-move loses track of a file | Durable journal intent before each move; reconciliation on start; sidecar manifests |
| Tampering | Restore overwrites a newer file at the original path | No-replace rename; conflicts go to the user |
| Information disclosure | Quarantine store indexed or backed up to the cloud | Excluded from Spotlight/Windows Search and from backups where the OS allows; documented |

### TB4: Core → privileged helper (future)

| Threat | Example | Controls |
| --- | --- | --- |
| Spoofing | Another process connects to the helper | XPC peer code-signing requirement; pipe DACL, `FILE_FLAG_FIRST_PIPE_INSTANCE`, remote clients rejected, client signature check |
| Elevation of privilege | Confused deputy: helper deletes a root-owned file named by an attacker | Helper accepts only signed plan IDs, re-checks deny-list and identities itself, operates by handle, never derives the user from its own environment ([ADR-0005](../decisions/0005-process-topology-and-privilege.md)) |
| Tampering | Version skew between app and helper | Versioned handshake; refuse on mismatch |

### TB5: Cloud Jev

| Threat | Example | Controls |
| --- | --- | --- |
| Information disclosure | Provider retains metadata | Opt-in, redaction, documented provider data policy; zero-data-retention status confirmed before launch |
| Tampering | Response from an unexpected model or a refusal parsed as data | Pinned model ID check, `stop_reason` handling, schema validation, unknown evidence IDs rejected |
| Spoofing | API key theft from local storage | Keys stored in the OS keychain or credential manager; never logged |

### TB6: Developer tool commands

| Threat | Example | Controls |
| --- | --- | --- |
| Elevation of privilege | `PATH` hijack runs a malicious `docker` binary | Resolve binaries from known install locations; verify signatures where possible; never run elevated |
| Tampering | Tool output misparsed into a false success | Measure free space before and after; record exit status; tolerate locale and version differences |

### TB7: Supply chain and CI

| Threat | Example | Controls |
| --- | --- | --- |
| Tampering | Compromised dependency (axios 2026-03, arrayref 2026-08) | Committed lockfiles, `--locked`, cargo-deny, OSV-Scanner, pnpm release-age gating, review of new packages, cargo-vet for sensitive crates |
| Tampering | Compromised action or tag force-push (Trivy 2026-03) | Actions pinned by SHA, zizmor, actionlint, harden-runner, no `pull_request_target` |
| Elevation of privilege | PR job reads secrets or poisons caches | Minimal permissions; caches saved only on `main`; secrets only in the protected release environment |
| Spoofing | Users install a tampered build | Code signing, notarization, artifact attestations, immutable releases |

## 5. Privacy (LINDDUN)

| Category | Concern | Control |
| --- | --- | --- |
| Linking / identifying | Content hashes and app lists fingerprint a user | Hashes and inventories never leave the device by default |
| Detecting | Telemetry reveals the presence of sensitive apps | No paths, names or app lists in metrics; telemetry opt-in |
| Disclosure | Exported diagnostics contain paths | User reviews the export; redaction layer on by default |
| Unawareness | Users don't know what cloud Jev sends | Disclosure screen with an example payload; per-user opt-in |
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
    ├── 4.1 Cloud sync propagates a deletion                           → sync roots KEEP; iCloud Photos warning
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

## 8. Security testing obligations

- Race tests (symlink/junction flips mid-execution) on macOS, Windows and Linux CI.
- A malicious-name corpus (control, bidi, newline, invalid UTF-8, NFC/NFD and case
  twins, long paths).
- IPC peer-rejection tests per transport.
- An injection canary corpus for Jev, in both directions.
- Fuzzing of every parser of untrusted data (plist, registry exports, package
  manifests, `getattrlistbulk` buffers).
- Supply-chain checks as required CI gates
  ([ADR-0026](../decisions/0026-ci-and-supply-chain-security.md)).
