# ADR-0063: Move apps owned by another account into quarantine through a privileged helper

- Status: Accepted (owner decision, 2026-11-08)
- Date: 2026-11-08
- Amends: [ADR-0005](0005-process-topology-and-privilege.md) (brings the opt-in
  privileged helper forward, for this action only),
  [ADR-0060](0060-list-software-and-remove-it-only-through-its-own-mechanism.md)
  (rule 6 and "no privileged helper in this epic"),
  [ADR-0062](0062-uninstall-macos-apps-into-quarantine.md) (rule 1, the owner check)
- Issue: #809; roadmap 16.3

## Context and problem statement

After 0.10.0 the owner asked (2026-11-08) for apps owned by another account to be
movable too. On a Mac those are apps installed by an administrator installer (`.pkg`)
or by the App Store: their bundles are owned by root, and only root may rename them.
ADR-0062 refuses them (`not_owned_by_user`), because ADR-0060 keeps Lumen to the
user's own rights until a reviewed helper exists.

The owner chose to use administrator rights through a **privileged helper** that moves
such apps into Lumen's quarantine. Two other mechanisms were considered:

- **The system's Move to Trash.** Finder asks for the administrator password and no
  Lumen code runs as root, but the app lands in the Trash, outside Lumen's quarantine.
- **Running one command as root with the administrator password each time.** It needs
  a new way to start Lumen code as root, and still has to re-check everything there.

A privileged component is the classic target for local privilege escalation. A
confused deputy can be steered by links, swapped files or a forged request into acting
on something else, so the rules below are the point of this ADR.

## Decision

1. **What it moves.** A `.app` bundle directly in `/Applications` that passes every
   ADR-0062 check except "owned by the account Lumen runs as":
   - not protected (not under `/System`, not SIP-protected, not on the sealed system
     volume);
   - no linked background item;
   - none of the helper folders in `Contents/Library`;
   - not running;
   - a quarantine store on its volume.

   Its owner is root or another account. Security software stays list-only (ADR-0060).
   Apps with helpers keep their own uninstallers. Nothing else is moved with
   administrator rights: system-scope cleanup stays read-only until its own ADR.
2. **The helper.** `lumen-helper` is a small Rust executable inside `Lumen.app`.
   - It is a launch daemon registered with `SMAppService.daemon`, available on macOS
     13 and later, which is Lumen's minimum.
   - It is **opt-in**: Lumen registers it only when the person first moves an app owned
     by another account, after saying what it is. macOS then asks the person to allow
     it (Login Items) and to authenticate as an administrator.
   - Lumen offers to turn it off in Settings (`unregister`), and removing the app
     removes it.
   - It runs on demand (launched by XPC) and exits when idle.
3. **Who may talk to it.**
   - It listens on one XPC Mach service.
   - It accepts a connection only from Lumen's own app: it sets a peer code-signing
     requirement on the connection (Developer ID, Lumen's team ID and bundle
     identifier). Any other client is refused.
   - It takes the target user from the connection's audit token (uid), never from its
     own environment, home or a field in the request.
4. **What it accepts.** Four verbs, each on one item of a plan the person confirmed by
   hash (ADR-0034); never a free path from the page:
   - `quarantine`: the planned bundle's path, identity and evidence hash;
   - `restore` and `finalize`: a held item's operation ID;
   - `locate`: for reconciliation.

   The request is untrusted input. The helper decides again, itself, on fresh
   evidence:
   - the ADR-0062 gate, with the owner rule above;
   - the identity (device and inode) and kind of the bundle;
   - that nothing uses it.

   As root it sees every process's open files, so the in-use answer must be `No`;
   ADR-0030's partial answer never applies here. Any mismatch refuses and leaves the
   app where it is.
5. **How it moves.**
   - It uses the reviewed executor, `lumen-fs-exec`, with no new destructive code: by
     handle from the parent folder, a no-replace rename into the store, then
     verification (ADR-0015, ADR-0016).
   - The store for these items is root-owned and private:
     `/Library/Application Support/Lumen/Quarantine/<uid>/`, mode 0700, on the same
     volume as `/Applications`. Only the helper moves items in or out.
   - Lumen's core keeps the journal and ledger as today; `restore`, `finalize` and
     reconciliation of these items go through the helper.
   - Finalizing permanently checks the whole held tree first, as for every item.
6. **What the person sees.** The preview says the app belongs to another account and
   that moving it uses Lumen's helper with administrator rights. The rest of the flow
   is unchanged: confirm by hash, restore until removed for good.
7. **Review and tests.**
   - The helper gets its own section in the threat model and a pass through the
     executor review checklist.
   - Its decision logic is tested unprivileged in temporary fixtures.
   - Registration, XPC and root moves need a signed build on a real Mac, so they are
     checked there before release and are not claimed from CI.
8. **Windows** is unchanged: no elevation. A later ADR may use ADR-0005's per-plan
   `runas` path.

## Consequences

- Good: apps installed by `.pkg` or the App Store can be moved and restored like the
  person's own, with the same preview and quarantine.
- Good: the helper is opt-in and narrow: four verbs, one kind of item, one caller, and
  everything re-checked as root.
- Bad: Lumen now ships a component that runs as root, a standing attack surface while
  it is registered. A bug in its checks could move something it should not; the
  rules above and the shared executor are the defence, and quarantine stays
  reversible.
- Bad: it depends on code signing. An unsigned or ad-hoc build cannot use it, and
  moving it to a new signing identity breaks the requirement until the helper is
  re-registered.
- Neutral: macOS's approval prompt for background items has varied between versions
  (Apple Developer Forums); the person may need to allow it in System Settings, and
  the app says where.

## Implementation (roadmap 16.3)

1. `feat(app)`: the Quit button (no privileges; separate from this ADR).
2. `feat(helper)`: `lumen-helper` skeleton:
   - registration through `SMAppService`, and the XPC listener with the peer
     code-signing requirement;
   - a `status` verb only;
   - its threat-model section.
3. `feat(helper)`: the root-owned store and the `quarantine` verb on `lumen-fs-exec`,
   with the gate re-run as root.
4. `feat(helper)`: `restore`, `finalize` and `locate`, and reconciliation through the
   helper.
5. `feat(web)`: the preview's owner note, turning the helper on and off in Settings.

Each step is checked on a signed build on a real Mac before the next merges.
