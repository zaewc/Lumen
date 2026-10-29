# Full Disk Access guidance is not permission status

- Date: 2026-10-29
- Trigger: the owner enabled Full Disk Access but Settings still appeared to ask for it.
- Finding: `PermissionsGroup` always displayed its setup instructions and system-settings
  button. It never queried the current grant. This was static help, not a detected denial.

## Decision

Keep the setup steps in a closed, keyboard-accessible disclosure. Explain that people
who already allowed Lumen can skip them and check the actual switch in macOS System
Settings. If they change it, guide them to quit completely, reopen and start a new scan;
past scan results do not change retroactively.

Do not replace static help with a guessed granted/denied badge. There is no public Full
Disk Access status API (ADR-0008). Probing a particular directory establishes access to
that directory only; a denial may also involve POSIX permissions, SIP, container
protection or a data vault. Reading the private TCC database or using private APIs is
not appropriate. The scanner's coverage and cleanup safety rules remain unchanged.

## Sources

- [Apple: Accessing files from the macOS App Sandbox](https://developer.apple.com/documentation/security/accessing-files-from-the-macos-app-sandbox):
  the person grants Full Disk Access in System Settings; filesystem access has other
  independent protection mechanisms and must still handle denial.
- [Apple DTS: Reliable test for Full Disk Access?](https://developer.apple.com/forums/thread/114452):
  there is no status API; the TCC database is not API, and Data Vaults may remain
  inaccessible even with Full Disk Access.
- [ADR-0008](../decisions/0008-capability-model-and-coverage.md):
  report unreadable locations as unknown, never empty, without inferring a global grant.
