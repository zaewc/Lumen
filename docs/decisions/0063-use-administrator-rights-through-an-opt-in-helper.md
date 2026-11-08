# ADR-0063: Use administrator rights through an opt-in helper, so permissions never stand in the way

- Status: Accepted (owner decisions, 2026-11-08)
- Date: 2026-11-08
- Amends:
  - [ADR-0005](0005-process-topology-and-privilege.md): the opt-in privileged
    helper is brought forward, on macOS and Windows;
  - [ADR-0060](0060-list-software-and-remove-it-only-through-its-own-mechanism.md):
    rule 6, and "no privileged helper in this epic";
  - [ADR-0062](0062-uninstall-macos-apps-into-quarantine.md): rule 1, the owner check.
- Issue: #809; roadmap 16.3 and 16.10

## Context and problem statement

After 0.10.0 the owner set a goal (2026-11-08): **permissions must never make cleaning
up apps or files awkward for the person.** Today Lumen runs only with the person's
rights (ADR-0005), so it cannot clean up two kinds of thing:

- **Apps owned by another account:**
  - on a Mac, apps installed by `.pkg` or the App Store, whose bundles are owned by
    root (ADR-0062 refuses them as `not_owned_by_user`);
  - on Windows, per-machine programs.
- **System-scope files**, such as machine-wide caches and logs. Lumen shows them
  read-only, with a note to remove them yourself.

The owner decided:

- **Mechanism.** A privileged helper that moves items into Lumen's quarantine, on
  macOS. On Windows, a service installed once rather than an elevation prompt every
  time.
- **Signing on Windows.** Lumen's own signing key, at no cost, rather than a bought
  certificate. Artifact Signing (formerly Trusted Signing) is open only to
  organizations, and to individuals in the United States or Canada.
- **Scope.** Apps, and system-scope files that the safety policy allows; never the
  personal folders of other accounts.
- **Timing.** The helper is set up the first time it is needed.

Rejected alternatives:

- the system's Move to Trash: outside Lumen's quarantine;
- one root command per action: needs a new way to start Lumen code as root, and still
  has to re-check everything there;
- a prompt for every action on Windows: the friction the goal rules out.

A privileged component is the classic target for local privilege escalation. A
confused deputy can be steered by links, swapped files or a forged request into acting
on something else. The rules below are the point of this ADR.

## Decision

### 1. Administrator rights remove the permission barrier, never a protection

The deterministic policy decides what may be cleaned, exactly as today. The helper
only supplies the rights to act on what it allows. Everything that is protected stays
protected, whoever owns it:

- system files and OS components: on macOS, SIP-protected paths and the sealed system
  volume; on Windows, files owned by TrustedInstaller or under Windows Resource
  Protection;
- security software, which stays list-only (ADR-0060);
- unknown application data, databases, credentials and keys, browser profiles;
- files in use, and cloud placeholders.

**Never touched:** the personal folders of other accounts on the computer (other
users' homes or profiles), whatever they hold.

### 2. What the helper may do

- **Apps owned by another account.**
  - **macOS:** a `.app` bundle directly in `/Applications` that passes every
    ADR-0062 check except the owner check. Apps with helpers keep their own
    uninstallers.
  - **Windows:** per-machine programs through their own mechanism (ADR-0060). An MSI
    product code is removed with `msiexec /x {ProductCode}`, run by the helper
    without a window.
  - A vendor uninstaller that needs its own window cannot run in the service's
    session. Lumen starts it with one elevation prompt, the vendor's own UI anyway.
- **System-scope files** the policy allows: machine-wide caches, logs and update
  leftovers outside every account's personal folder.
  - Each location needs a rule with a cited source, as every policy rule does. Until
    a location has one, it stays read-only.
  - These are moved into quarantine like the person's own files, never deleted
    directly.

### 3. The helper

`lumen-helper` is a small Rust executable that uses the reviewed `lumen-fs-exec`, so
there is no new destructive code.

**On macOS:**
- It is a launch daemon registered with `SMAppService.daemon` (macOS 13 and later,
  Lumen's minimum).
- macOS asks the person to allow it (Login Items) and to authenticate as an
  administrator.
- It runs on demand through XPC and exits when idle.

**On Windows:**
- It is a service with demand start, installed once with one elevation prompt.
- The installer step checks the helper's signature, then copies it to
  `%ProgramFiles%\Lumen Helper\`. Only administrators can write there, so the person's
  per-user install folder never holds code that runs as SYSTEM.
- The service runs as LocalSystem with the fewest privileges it needs (a service SID
  and a restricted privilege list), and stops when idle.

**On both:**
- **Opt-in.** It is set up the first time the person chooses something that needs it,
  after Lumen says what it is.
- It can be turned off in Settings, which unregisters or removes it. Uninstalling
  Lumen removes it.
- **Signed.** It requires signed builds:
  - macOS: Developer ID, already in place;
  - Windows: **Lumen's code key**, rule 8.

  A build without these signatures cannot use it.

### 4. Who may talk to it

**macOS:**
- One XPC Mach service, with a peer code-signing requirement: Lumen's Developer ID
  team and bundle identifier.
- The target user comes from the connection's audit token.

**Windows:**
- One named pipe whose DACL allows only the account that installed the helper.
- The service impersonates the client (`ImpersonateNamedPipeClient`) and takes the
  user's SID from that token, never from its own environment. Under Windows 11's
  Administrator protection, an elevated process runs as a separate hidden account and
  profile.
- It checks that the client process's image file carries a valid signature by Lumen's
  code key (rule 8).

**Never from the request:** the target user is never read from a field in the request,
nor from the helper's own `HOME`, `HKCU` or `%TEMP%`.

### 5. What it accepts

Only items of a plan the person confirmed by hash (ADR-0034), never a free path from
the page. Its verbs:

- `quarantine` (a planned item's path, identity and evidence hash);
- `restore`, `finalize` and `locate` (a held item's operation ID);
- `remove_package` (Windows: a recognised MSI product code from the inventory);
- `status`.

The request is untrusted input. The helper decides again, itself, on fresh evidence:

- the policy rule or the ADR-0062 gate that admits the item;
- the item's identity and kind;
- the protections in rule 1;
- that the item is not in any account's personal folder;
- that nothing uses it. With full rights it sees every process, so the in-use answer
  must be `No`; ADR-0030's partial answer never applies.

Any mismatch refuses and leaves the item where it is.

### 6. Where items go

Items are moved, by handle and without replacing (ADR-0015, ADR-0016), into a store
private to the helper on the same volume:

- macOS: `/Library/Application Support/Lumen/Quarantine/<uid>/`, mode 0700;
- Windows: `%ProgramData%\Lumen\Quarantine\<SID>\`, with a DACL for SYSTEM and
  Administrators only, on the system drive. Other drives follow ADR-0046's rules in a
  later step.

Lumen's core keeps the journal and the ledger as today. Restore, finalize and
reconciliation of these items go through the helper. Finalizing checks the whole held
tree first.

### 7. What the person sees

- The first time, one explanation and the system's own approval: macOS Login Items and
  administrator authentication, or one Windows elevation prompt.
- After that, no permission prompts. The preview says when an item belongs to another
  account or to the system and is handled by Lumen's helper.

### 8. Lumen's code key on Windows

Windows releases are not Authenticode-signed. A certificate costs money every year,
and Artifact Signing is not open to individuals outside the United States and Canada.
The helper does not need public trust; it needs to know a file came from Lumen's
release. So Windows uses **Lumen's own code key**:

- **The key.** A minisign (Ed25519) key used for nothing else. It is separate from the
  updater key (ADR-0048), so a signature for one purpose can never pass for the other.
  - The private key lives only in the `release` environment.
  - Only the `sign` job uses it, with network access blocked. No build job ever
    receives it.
  - Its public key is pinned in the helper's source.
- **What is signed.** In the release workflow, `Lumen.exe` and `lumen-helper.exe` are
  signed after they are built, then packaged into the installer with their detached
  signatures. Packaging moves after signing, as macOS signing already does. Each
  signature's trusted comment names the file and the version.
- **What is verified.** Verification uses the `minisign-verify` crate, already in the
  dependency tree for the updater.
  - **Before installing.** The elevated install step verifies `lumen-helper.exe`
    against the pinned key. It then copies the file to `%ProgramFiles%` and verifies
    the copy again.
  - **On every connection.** The helper verifies the client's image file.
- **What it does not do.**
  - Windows still shows "unknown publisher" and SmartScreen warnings when Lumen is
    downloaded and installed, as it does today. A bought certificate would change only
    that, and can be added later without changing the helper.
  - On Windows, a process running as the same user can inject code into a genuine,
    signed `Lumen.exe`. The client check therefore keeps other programs and other
    accounts from using the helper casually, but it is not the security boundary,
    and the same is true of Authenticode. The boundary is rules 1 and 5: the helper
    re-decides everything itself, acts only on what the policy allows, and every move
    is a reversible quarantine.
- **Rotation.** A new key means a new helper with the new pinned key. Lumen sets the
  helper up again the next time it is needed. A key that leaks is replaced at once,
  and the release that replaces it says so.

### 9. Review and tests

- The helper gets its own threat-model section and a pass through the executor review
  checklist.
- Its decision logic is tested unprivileged in temporary fixtures.
- Registration, IPC and privileged moves need a signed build on a real Mac or PC.
  They are checked there before release and never claimed from CI.

## Consequences

- Good: apps and system-scope files that the policy allows are cleaned up like the
  person's own, with the same preview, quarantine and restore, and no permission
  prompts after the first.
- Good: the helper is opt-in and narrow: a few verbs, one caller, and everything
  re-checked with full rights.
- Bad: Lumen ships a component that runs as root or SYSTEM, a standing attack surface
  while it is installed. A bug in its checks could move something it should not; the
  rules above and the shared executor are the defence, and quarantine stays
  reversible.
- Bad: it depends on code signing: Developer ID on macOS, and Lumen's own code key on
  Windows. A new signing identity breaks the client check until the helper is set up
  again.
- Bad: Windows keeps its "unknown publisher" warnings, because Lumen's code key is not
  publicly trusted.
- Neutral: macOS's approval prompt for background items has varied between versions.
  The app says where to allow it in System Settings.

## Implementation (roadmap 16.3 and 16.10)

**Prerequisite (owner):** Lumen's code key, generated once and stored in the
`release` environment. The steps are in
[`docs/operations/release.md`](../operations/release.md), under "Lumen's code key".

**macOS:**
1. The Quit button. It needs no privileges and is separate from this ADR.
2. The `lumen-helper` skeleton: `SMAppService` registration, XPC with the peer
   requirement, and `status`; plus its threat-model section.
3. The root-owned store, and `quarantine` for apps owned by another account, with the
   gate re-run as root.
4. `restore`, `finalize`, `locate`, and reconciliation through the helper.
5. Web: the preview's owner note, and the Settings switch.

**Windows** (after the code key exists):
6. The release pipeline: sign `Lumen.exe` and `lumen-helper.exe` with Lumen's code
   key in the `sign` job, then package the installer. Then the service: install with
   one elevation prompt, the copy to Program Files, the pipe with its DACL and client
   checks, and `status`.
7. The store, and `quarantine`, `restore`, `finalize` and `locate`.
8. `remove_package` for MSI product codes, and the one-prompt path for vendor
   uninstallers with a window.

**System-scope files** (16.10), on both platforms after the steps above: policy rules
with cited sources for each system-scope location, one location per PR, shown
read-only until its rule exists.

Each step is checked on a signed build on a real computer before the next merges.
