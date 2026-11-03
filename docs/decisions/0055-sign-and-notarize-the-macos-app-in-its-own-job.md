# ADR-0055: Sign and notarize the macOS app in its own job

- Status: Accepted (owner decision, 2026-11-03)
- Date: 2026-11-03
- Amends: [ADR-0038](0038-unsigned-beta-from-a-project-website.md) (the macOS app is no
  longer unsigned), [ADR-0006](0006-desktop-distribution-and-signing.md) (its macOS part
  is carried out; Windows is not)

## Context and problem statement

ADR-0038 shipped an unsigned beta because the owner had put the Apple Developer
Program membership on hold. The owner now has the membership and asked for signing
(roadmap 12.8). On macOS 15 and later an app that is not notarized cannot be opened
by Control-click; the person has to allow it in System Settings › Privacy & Security.
An ad-hoc signature also changes with every build, so macOS may ask for Full Disk
Access again after each update.

The owner is an individual in Korea. Azure Artifact Signing accepts individuals only
in the United States and Canada (organizations also in South Korea, among other
countries), so Windows signing needs another identity (a registered organization, or
an HSM-backed OV certificate) and is left for later by the owner's choice.

The release workflow keeps secrets away from code it does not control: the built-in
Jeb key reaches only the two compile steps, and the updater key only a job that runs
the locked Tauri CLI on a runner with egress blocked
([ADR-0048](0048-in-app-updates.md)). The Developer ID's private key is worth more
than either: anyone holding it can sign software as the owner for any Mac.

## Decision drivers

- People can open Lumen on macOS without a Gatekeeper block.
- The Developer ID certificate and the notary key never reach dependency build
  scripts, the web build or anything else installed from a registry.
- The disk image people download, and the app the updater installs, are the same
  signed and notarized app.
- A release that cannot be signed fails; it never goes out unsigned by accident.

## Considered options

1. **Tauri signs during the build:** set `APPLE_CERTIFICATE`, `APPLE_API_KEY` and the
   rest in the desktop job, and `tauri build` signs, notarizes and staples.
2. **A separate job signs the built app:** the desktop job builds ad hoc as today; a
   `macos-sign` job, with only the repository's script and Apple's tools, signs the
   app inside out with the Hardened Runtime, has Apple notarize it, staples it, puts
   it back into Tauri's disk image, and signs, notarizes and staples that.

## Decision outcome

Chosen option: **2**. Option 1 is simpler but puts the certificate in the same job as
every Cargo build script and npm package of the build, which is what ADR-0048 moved
the updater key away from.

- [`scripts/sign-macos.sh`](../../scripts/sign-macos.sh) does the signing. It signs
  every Mach-O file below `Contents` and then the bundle, with `--options runtime`
  and a secure timestamp, and no entitlements: Lumen needs none, and each one
  weakens the Hardened Runtime. A nested bundle (framework, helper app, XPC service)
  stops the run, because it needs its own signing order; Lumen ships none today.
- The certificate goes into a keychain made for the run with a random password and
  deleted at the end; the notary key (an App Store Connect API key) goes into a file
  readable only by the runner user and is deleted with it. The certificate must hold
  exactly one "Developer ID Application" identity.
- Notarization uses `notarytool submit --wait`; anything but `Accepted` prints Apple's
  log and fails the release. `spctl` must then accept the app and the disk image.
- The disk image keeps Tauri's layout: it is converted to a writable image, the app
  inside is replaced with the signed one, and it is compressed again. The updater's
  archive is made again from the same signed app, so the `sign` job signs (ADR-0048)
  what was notarized.
- Pull requests that change the workflow run the same script with an ad-hoc signature
  and no notarization, and read no secret. A release run without the five secrets
  fails.
- macOS runners cannot block egress (release.md); the job is audited instead, and
  installs nothing.

### Consequences

- Good: Gatekeeper opens the downloaded app without a warning; the signing identity
  stays the same from one release to the next, so macOS keeps Full Disk Access
  across updates.
- Good: the certificate is in one job that runs no third-party code.
- Bad: two notarization round trips (app and disk image) add a few minutes to each
  release, and an Apple outage delays a release.
- Bad: copies installed before the first signed release were signed ad hoc; their
  Full Disk Access grant does not carry over to the signed app, so it must be granted
  once more after that update. The release notes say so.
- Bad: Windows SmartScreen still warns about an unknown publisher until Windows is
  signed (ADR-0006).
- Neutral: the CLI archive for macOS is not signed yet; a follow-up signs and
  notarizes it the same way.

## More information

- [Releases: Signing the macOS app](../operations/release.md#signing-the-macos-app)
- Apple: [Notarizing macOS software before distribution](https://developer.apple.com/documentation/security/notarizing-macos-software-before-distribution),
  [Customizing the notarization workflow](https://developer.apple.com/documentation/security/customizing-the-notarization-workflow)
- Microsoft: [Artifact Signing quickstart, eligibility](https://learn.microsoft.com/en-us/azure/artifact-signing/quickstart)
