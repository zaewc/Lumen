# ADR-0038: Ship an unsigned beta from a project website

- Status: Accepted
- Date: 2026-10-25
- Amends: [ADR-0006](0006-desktop-distribution-and-signing.md) (when signing starts)

## Context and problem statement

ADR-0006 chose direct distribution outside the app stores, signed and notarized,
before the first public build. The App Store is not an option in any case: its
sandbox forbids the whole-disk view Lumen needs. The owner has put the Apple
Developer Program membership (and with it Developer ID signing and notarization) on
hold, but wants people to be able to download and use Lumen now, from a website of
its own.

## Decision drivers

- People can install and try Lumen without the owner buying signing identities first.
- Nobody is misled: every download says plainly that it is unsigned and how to open it.
- Downloads stay verifiable without a signing identity.
- Nothing here blocks moving to ADR-0006's signed builds later.

## Considered options

1. **Wait for signing** (ADR-0006 as written): no public build until Developer ID and a
   Windows signing identity exist.
2. **Unsigned beta from a project website:** a static site on GitHub Pages links to the
   GitHub release files and explains how to open an unsigned app; signing follows
   when the accounts exist.
3. **Unsigned files on the releases page only:** no website.

## Decision outcome

Chosen option: **2**.

- The site is static (GitHub Pages, built from this repository), states that the build
  is an unsigned beta, and links each platform's file from the latest GitHub release,
  with `SHA256SUMS` and the `gh attestation verify` command (ADR-0026).
- Releases stay drafts until the owner publishes them (docs/operations/release.md);
  versions are `0.x` while unsigned.
- ADR-0006 still describes v1: Developer ID, Hardened Runtime and notarization on
  macOS, and a signed installer on Windows, as soon as the owner provides them.

### Consequences

- Good: Lumen can be downloaded today; downloads remain verifiable through checksums
  and build-provenance attestations.
- Bad: on macOS 15 or later, Gatekeeper blocks the first launch until the user allows
  it in System Settings › Privacy & Security (Control-click no longer bypasses it);
  Windows SmartScreen warns about an unknown publisher.
- Bad: without a stable signing identity, macOS may treat an updated build as a new
  app, so Full Disk Access may have to be granted again after an update.
- Bad: no automatic updates; the Tauri updater waits for signed builds.

## More information

- [ADR-0006](0006-desktop-distribution-and-signing.md), [ADR-0026](0026-ci-and-supply-chain-security.md)
- [Releases](../operations/release.md)
