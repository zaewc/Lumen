# ADR-0006: Distribute desktop builds outside the app stores, signed and notarized

- Status: Proposed
- Date: 2026-10-06

## Context and problem statement

Lumen's desktop value depends on seeing other applications' caches, launch agents and
startup entries. The Mac App Store requires the App Sandbox, which forbids that, and
MSIX packaging on Windows virtualises the registry and AppData and restricts services
and elevation. Unsigned or unnotarized builds are effectively unusable: since macOS
Sequoia users cannot Control-click past Gatekeeper, and SmartScreen warns on unknown
publishers.

This decision is **Proposed** because it is also a business decision (store presence,
legal entity for signing).

## Decision drivers

- Full-reach, user-consented scanning on desktop.
- Trustworthy installation experience.
- Signing identity eligibility and long-term reputation.

## Considered options

1. Direct distribution: macOS Developer ID + Hardened Runtime + notarization;
   Windows signed MSI/NSIS installer with Azure Artifact Signing.
2. Store-only: Mac App Store (sandboxed) and MSIX in the Microsoft Store.
3. Both: full direct build plus reduced store "Lite" builds.

## Decision outcome

Recommended option: **1** for v1, with option 3 revisited after launch.

macOS:

- Developer ID signing, Hardened Runtime on the app and every nested executable,
  notarization with `notarytool` (App Store Connect API key in CI) and stapling.
- Minimum macOS 13 (`SMAppService`); universal binary while macOS 26 on Intel is
  supported.
- `NSAppDataUsageDescription` for the macOS 14+ other-app-container consent prompt.

Windows:

- NSIS `currentUser` (or MSI) installer, every PE signed with Azure Artifact Signing
  and RFC 3161 timestamps; one signing identity kept long-term to build SmartScreen
  reputation. If the owner is not eligible (individuals: US/Canada only), fall back to
  an HSM-backed OV certificate (max 460-day validity since 2026-03-01).
- Minimum Windows 10 22H2 and Windows 11 24H2+.
- No MSIX-only build. A Store listing, if wanted, uses policy 10.2.9 (MSI/EXE) and
  excludes any NT service.

Updates use the Tauri updater with offline-stored signing keys.

### Consequences

- Good: full product capability; no sandbox compromises.
- Bad: requires an Apple Developer Program membership and a Windows signing identity
  before the first public build.
- Bad: no store discovery in v1.

## More information

- [macOS research, Summary and Implication 1](../research/03-macos-platform.md)
- [Windows research, §I](../research/04-windows-platform.md)
- [Desktop research, Implication 7](../research/01-desktop-architecture.md)
- Owner questions: legal entity and country for signing; whether a store "Lite" SKU
  is wanted.
