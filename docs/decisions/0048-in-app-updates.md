# ADR-0048: In-app updates with Tauri's updater

- Status: Accepted
- Date: 2026-10-28
- Refines: [ADR-0026](0026-ci-and-supply-chain-security.md) (release artifacts)

## Context and problem statement

Each Lumen release is a new download: people uninstall and reinstall, or install over
the old copy by hand (and, on macOS, allow the unsigned app again). The owner asked for
updates inside the app (2026-10-28). Releases are built by `release.yml` on a `v*` tag,
published on GitHub Releases, and verified by checksums and build-provenance
attestations; the apps are not code-signed yet (roadmap 12.8).

## Decision drivers

- An update replaces the program that moves the user's files: it must come only from
  Lumen's releases and be verified before it runs.
- Nothing installs without the user asking; the app keeps working when there is no
  network or no update.
- No new server: GitHub Releases already hosts the files.
- Prefer maintained, official components over custom installers.

## Considered options

1. **Tauri's official updater plugin** (`tauri-plugin-updater`), with update artifacts
   signed by a Lumen-owned minisign key and a static `latest.json` on GitHub Releases
   (the decision below).
2. **A custom updater** (download with `ureq`, verify, swap the app). Rejected: it
   would re-implement per-platform installation (the `.app` bundle swap, the NSIS
   installer, AppImage replacement) that the plugin already does and maintains.
3. **Only tell the user a new version exists** and open the download page. Rejected by
   the owner's request; kept as the fallback for packages the updater cannot replace
   (`.deb`).

## Decision outcome

Chosen option: **1**.

- **Dependency:** `tauri-plugin-updater` 2.14.0 (MIT or Apache-2.0, maintained by the
  Tauri team, published 2026-10-14), desktop only. It verifies each download against
  the public key compiled into the app (minisign, Ed25519) before installing.
- **Signing key:** an updater key pair separate from code signing. The public key is in
  `tauri.conf.json`; the private key and its password are secrets of the `release`
  environment (`TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`), given
  only to the tag-run desktop build, like the built-in Jeb key (ADR-0042). The owner
  keeps an offline backup: losing the key means existing installs can no longer be
  updated.
- **Artifacts:** `bundle.createUpdaterArtifacts` makes the macOS `.app.tar.gz`, and
  signatures for it, the NSIS installer and the AppImage. The release job writes
  `latest.json` (version, notes link, date, and per platform the URL and signature:
  `darwin-aarch64` and `darwin-x86_64` both point at the universal archive) and
  attaches it, its checksum and attestation with the rest.
- **Endpoint:** `https://github.com/zaewc/Lumen/releases/latest/download/latest.json`,
  HTTPS only. Drafts are never served there, so only a published (verified) release is
  offered.
- **In the app:** the Rust host checks at start and from Settings; the page shows "a
  new version is available" with the version and a link to its notes, and installs
  only when the user presses Update, then restarts. The check and install are Lumen's
  own commands (no plugin permission is granted to the page). Settings shows the
  current version and a Check for updates button. A `.deb` install is offered the
  download page instead.
- **Failure:** a failed check is silent at start and reported in Settings; a failed
  download or a bad signature installs nothing.

### Consequences

- Good: people update with one click; a tampered or foreign file is refused by the
  signature check.
- Bad: a new dependency tree (an HTTP client) in the desktop host, reviewed by
  cargo-deny and OSV like the rest.
- Bad: the private key is a new secret to protect and back up.
- Neutral: an archive the app downloads itself is not marked as quarantined by macOS
  the way a browser download is, so an unsigned build should update without the
  "allow anyway" step; the first real update confirms it. Code signing (12.8) stays a
  separate step.
