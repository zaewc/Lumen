# Lumen public-beta readiness audit

**Audit snapshot:** filesystem, UX and supply-chain review at `bf29007`; platform
reliability, AI/privacy and test/quality review at `b5abb7d` (2026-10-31). The
latter includes the merged changes from #565–#567. Findings marked as risks or
untested concerns are not confirmed incidents.

## Summary

No P0 issue was identified. The filesystem executor's handle-relative checks,
no-follow behavior, no-replace moves, restore safeguards, journal ordering and
fail-closed policy have substantial test coverage. The most important release
blockers are cloud-sync locations being treated as reversible, insufficient
host-level confirmation for person-chosen cleanup, Linux whole-disk scans that
do not hard-protect system paths, and configured cloud AI requests that use a
redaction marker instead of their authentication key.

Do not merge #574 or #575 as written. The filesystem review found #571 and #572
safe to merge in order, and #573 safe with follow-up work. CI passed for
#571–#573 at the time checked. #574 requires host-rendered confirmation before
person-chosen items can be executed or permanently finalized. #575 requires
cloud-sync protection and protection for ancestors of sensitive locations.

## P1 — Resolve before broad public distribution

### FS-1: Webview-only confirmation can authorize irreversible actions

**Evidence:** The desktop host accepts a plan hash returned to the webview as
confirmation; `finalize` and irreversible tool commands are also invoked
through webview IPC. PR #574 makes the webview the selector for up to 10,000
review items, while the host does not independently show a confirmation
([`commands.rs`](../../apps/desktop/src-tauri/src/commands.rs),
[`isolation/index.js`](../../apps/desktop/isolation/index.js)).

**Risk:** A compromised webview could select and quarantine many review items,
then finalize them. CSP and isolation reduce likelihood but do not provide a
host-side authorization boundary.

**Action:** Block #574 until destructive operations have a native confirmation
whose content is built by the host, or equivalent host-enforced authorization.
Correct the threat-model statement that the host prevents select-all.

### FS-2: Cloud-synced folders can be moved out of their sync domain

**Evidence:** Cloud protection currently checks flags on the candidate itself;
there is no sync-root ancestor check. A materialized folder below iCloud Drive,
Dropbox, Google Drive or OneDrive can therefore be offered and renamed into
quarantine. Moving it out of the sync root can propagate deletion to the
provider and other devices
([`collect.rs`](../../crates/lumen-application/src/collect.rs),
[`rules.rs`](../../crates/lumen-policy/src/rules.rs)). PR #575 permits
selection of large unrecognized folders, widening the exposure.

**Action:** Block #575 until candidates below sync roots and directories with
cloud placeholders are kept or excluded. Add platform-specific ancestry
detection and tests for sync-root descendants and placeholder descendants.

### PLATFORM-1: Linux whole-disk scans do not hard-protect system paths

**Evidence:** Linux whole-disk scanning uses `/` as a root; the policy tests
confirm `/usr/lib` has no Linux protection rule. Unknown large items can reach
`REVIEW`, and Linux supports quarantine rename
([`roots.rs`](../../crates/lumen-app/src/roots.rs),
[`locations.rs`](../../crates/lumen-policy/src/locations.rs),
[`capability.rs`](../../crates/lumen-domain/src/capability.rs)).

**Risk:** This is not an automatic deletion path, but system paths can be
presented for user selection.

**Action:** Add sourced Linux system-path protections and fixtures, or limit
Linux whole-disk scanning until those protections exist.

### UX-1: Partial scans can show a previous scan's results

**Evidence:** A scan that stops early is recorded as cancelled, while assessment
uses the latest completed scan because the transport does not pass a scan ID.
The UI can then label old results as belonging to the partial scan
([`scan.rs`](../../crates/lumen-app/src/scan.rs),
[`index.rs`](../../crates/lumen-store-sqlite/src/index.rs),
[`session.tsx`](../../apps/web/src/entities/scan-session/model/session.tsx)).

**Action:** Bind assessment to an explicit completed scan ID. Render a stopped
state with no candidates or cleanup action and show the out-of-space guidance.

### UX-2: Scans and assessments cannot be cancelled from the desktop

**Evidence:** The desktop passes a fresh cancellation token that is never
cancelled, and the transport/UI has no cancellation operation
([`commands.rs`](../../apps/desktop/src-tauri/src/commands.rs),
[`transport.ts`](../../apps/web/src/shared/api/transport.ts)).

**Action:** Wire a per-run cancellation token through host and UI; make cancelled
scans visibly distinct and ensure they cannot be assessed as complete.

### UX-3: Interrupted cleanup can hide items until another cleanup runs

**Evidence:** Reconciliation is invoked during a later quarantine operation,
not on desktop startup. Journaled or moved items may not appear in the held
items list after a crash or quit
([`quarantine.rs`](../../crates/lumen-app/src/quarantine.rs),
[`lib.rs`](../../apps/desktop/src-tauri/src/lib.rs)).

**Action:** Reconcile on startup, surface items needing attention, and prevent
quit/update during cleanup or quarantine actions. Add a desktop recovery test.

### SC-1: Previously exposed updater signing key was not rotated

**Evidence:** Historical release builds exposed the updater signing key to
build-time code with network access; the trusted public key had not changed at
the time of review. There is no evidence that the key was stolen
([`release.yml`](../../.github/workflows/release.yml),
[`tauri.conf.json`](../../apps/desktop/src-tauri/tauri.conf.json)).

**Action:** Treat key rotation as a release prerequisite. Ship a transition
release trusted by the old key, then use a new key; document the rotation and
test rejection of an old-key signature after transition.

### RELEASE-1: Public installer signing and AI-service authorization need a
release decision

**Evidence:** macOS and Windows installers are not code-signed/notarized, and
the public release workflow embeds `JEB_API_KEY`. Repository documentation
requires authorization from the AI Hub operator, but the audit did not establish
whether that approval or abuse limits are in place.

**Action:** Complete platform signing/notarization or explicitly constrain beta
distribution. Confirm written authorization and quotas for the embedded
service key; otherwise omit it from public builds.

### AI-1: Cloud AI authentication sends a placeholder instead of the API key

**Evidence:** The shared OpenRouter transport sends `Authorization: ******`;
its test expects that literal placeholder. Both Jeb and OpenRouter use the
transport
([`openrouter.rs`](../../crates/lumen-jev/src/openrouter.rs)).

**Impact:** Configured cloud judges cannot authenticate, so requests fail and
the app continues without Jev evidence. This does not let the AI choose actions.

**Action:** Send the configured bearer credential without logging it. Update
the test to verify header construction without expecting a placeholder, and
verify that failures are surfaced as unavailable AI evidence.

## P2 — Close before or shortly after beta, with owners and dates

- **FS-3:** Directory candidates can contain protected locations, credentials
  or browser profiles without being protected themselves. Make protected
  ancestor checks and browser-profile coverage prerequisites for #575.
- **FS-4/5:** Directory contents can drift after scan, and the executor record
  is not fully bound to the planned identity. Add identity and subtree-change
  tests before widening person-selected folder cleanup.
- **FS-6/7:** Startup reconciliation and recovery from a lost/corrupt ledger are
  absent. Surface interrupted operations and define a sidecar/ledger recovery
  strategy.
- **FS-8/9:** A failed restore may remain eligible for automatic removal;
  retention depends on the wall clock. Add a safety test and make failed restore
  items ineligible for expiry; test clock anomalies.
- **ANDROID-1:** The Android photo permission flow does not represent
  user-selected/limited photo access. Request the applicable selected-photo
  permission and expose the limited coverage state
  ([`app.json`](../../apps/mobile/app.json),
  [`index.ts`](../../apps/mobile/modules/lumen-photos/index.ts)).
- **MOBILE-1:** The mobile photo screen synchronously enumerates and retains the
  full photo library. Make loading incremental, asynchronous and cancellable
  ([`ReviewGrid.tsx`](../../apps/mobile/src/ReviewGrid.tsx)).
- **RELEASE-2:** Release packaging does not assert that every advertised `.dmg`
  and `.deb` exists; add explicit completeness checks.
- **QUALITY-1:** CodeQL analyzes Rust and Actions but not JavaScript/TypeScript.
  Add TypeScript analysis
  ([`codeql.yml`](../../.github/workflows/codeql.yml)).
- **QUALITY-2:** The threat model calls for parser fuzzing, but checked-in fuzz
  targets and a CI fuzz smoke run were not found. Add reproducible fuzz coverage
  or document the owner-run process
  ([`threat-model.md`](./threat-model.md),
  [`rust.yml`](../../.github/workflows/rust.yml)).
- **AI-2:** Judgment and description traces define a response hash, but both
  paths store `None`. Hash exact response bytes before validation and retain
  only the digest in the trace
  ([`judgment.rs`](../../crates/lumen-domain/src/judgment.rs),
  [`consult.rs`](../../crates/lumen-application/src/consult.rs),
  [`describe.rs`](../../crates/lumen-application/src/describe.rs)).
- **AI-3:** After the user enables AI review, requests include the basename and
  redacted display path. The current disclosure permits provider retention but
  does not establish a retention guarantee. Confirm processing/retention terms
  before beta; consider omitting or further redacting these fields. This is a
  residual data-minimization risk, not evidence of a consent bypass
  ([`request.rs`](../../crates/lumen-jev/src/request.rs),
  [`privacy.md`](../operations/privacy.md)).
- **UX-4/5:** Bulk permanent-removal confirmation can remain armed after the
  selection changes, and bulk results can misstate what happened. Reset
  confirmation on selection changes and accurately report per-item outcomes.
- **UX-6/7:** Cleanup confirmation does not list affected items; irreversible
  tool commands lack an adequate preview/focus treatment. Show actionable
  details or clearly state when no preview exists.
- **UX-8/9/10/11:** Add redacted diagnostics, backend concurrency guards,
  accessible progress announcements, and UI/accessibility tests for safety
  flows.
- **SC-2/3/5/6/7:** Exercise the new signing pipeline on a real tag before
  publication; enable immutable releases; ship third-party license notices;
  reconcile release controls in ADR-0026 with actual enforcement; and enable
  dependency update automation.

## P3 — Track as follow-up

- UX: persist the last completed scan, localize safety warnings, add a native
  folder picker or `~` expansion, make retry prepare a new plan, and expose
  disabled-action reasons accessibly.
- Supply chain: isolate signing secrets in a dedicated environment, tighten
  Actions settings, document accepted unsigned-platform limitations, monitor
  the embedded AI key, and include system libraries in AppImage SBOMs.
- Executor durability: test Windows fault-injection paths and macOS durable
  flush behavior.

## Validation and limitations

- No builds or tests were run as part of the read-only audits.
- #571 passed CI and merged as `8f32014` (#571). Its branch deletion closed
  stacked PR #572 automatically; #572 was reopened, retargeted to `main`, and
  merged as `37c5c07` after its fresh checks passed.
- #573 was rebased onto current `main`. That exposed an outdated test fixture
  missing the `free_when_closed` field added by #565; the fixture was updated,
  its three planning unit tests and the local pre-push Clippy hook passed, and
  GitHub CI is pending. #574 and #575 remain blocked by the findings above.
- Filesystem, UX and supply-chain observations were initially made against
  `bf29007`; re-check any implementation-level finding against current `main`
  before fixing it.
- Android limited-photo access is a capability/coverage issue; the audit did
  not establish data loss.
