# Releases

> [ADR-0026](../decisions/0026-ci-and-supply-chain-security.md): release provenance
> users can verify. Workflow: [`release.yml`](../../.github/workflows/release.yml).

## What a release contains

| File | What it is |
| --- | --- |
| `lumen-aarch64-apple-darwin.tar.gz` | the `lumen` command line for Apple silicon Macs |
| `lumen-x86_64-unknown-linux-gnu.tar.gz` | for 64-bit Linux with glibc 2.39 or later (built on Ubuntu 24.04) |
| `lumen-x86_64-pc-windows-msvc.zip` | for 64-bit Windows |
| `Lumen_<version>_universal.dmg` | the desktop app for macOS 13 or later, Apple silicon and Intel |
| `Lumen_<version>_x64-setup.exe` | the desktop app's per-user Windows installer (no administrator rights needed) |
| `Lumen_<version>_amd64.deb`, `Lumen_<version>_amd64.AppImage` | the desktop app for 64-bit Linux |
| `lumen-<target>.cdx.json` | a CycloneDX 1.5 SBOM per target: every crate compiled into that binary, from `Cargo.lock` |
| `lumen-desktop-<target>.cdx.json` | the same for the desktop app's Rust host |
| `lumen-desktop-web.cdx.json` | the npm packages bundled into the desktop app's interface, from `pnpm-lock.yaml` |
| `SHA256SUMS` | SHA-256 checksums of every file above |

Each archive holds the binary, `LICENSE` and `README.md`. Every file listed in
`SHA256SUMS` carries a signed SLSA build-provenance attestation, which ties it to the
workflow run, commit and tag that built it.

Nothing is **signed** yet, so macOS Gatekeeper and Windows SmartScreen warn about
these files. The macOS app is only ad-hoc signed, so Apple silicon will run it, but
Gatekeeper still blocks it until it is allowed in System Settings › Privacy &
Security. Signing and notarization are roadmap 12.8.

## Cutting a release

1. Set `version` in the root `Cargo.toml` and in
   `apps/desktop/src-tauri/tauri.conf.json` to the new version, in its own pull
   request.
2. Once that is merged, tag the merge commit `v<version>` and push the tag. The build
   fails if the tag does not match either version.
3. The workflow builds each target, writes the SBOMs and checksums, attests them and
   creates a **draft** release with generated notes.
4. Review the draft (files, checksums, notes), then publish it.

The build jobs can read the repository and nothing else; only the attest job can mint
a signing token, and only the release job can write releases. Neither runs repository
code. Release builds use no cache. On Linux, the CLI build and SBOM jobs may reach only the
toolchain, crates.io, npm, GitHub and artifact storage; the desktop job audits its
network use while its package mirrors are pinned down. The macOS and Windows runners
cannot enforce that limit.

A pull request that changes the workflow runs the builds as a dry run; nothing is
attested or published.

## Verifying a download

```sh
sha256sum -c SHA256SUMS --ignore-missing
gh attestation verify lumen-x86_64-unknown-linux-gnu.tar.gz --repo zaewc/Lumen
```

The second command checks that the file was built by this repository's release
workflow. It needs the GitHub CLI.

## Download site

[ADR-0038](../decisions/0038-unsigned-beta-from-a-project-website.md): the public
download page, served by GitHub Pages at <https://zaewc.github.io/Lumen/>.

- **What it is:** a static page in [`apps/site`](../../apps/site) with no build step and
  no dependencies: `index.html` (Korean and English), `style.css`, `lang.js` (picks the
  language before the page is drawn) and `main.js`. It loads nothing from other sites
  and has no cookies or analytics; its Content-Security-Policy allows connections to
  `api.github.com` only.
- **What it shows:** the release GitHub marks as **Latest**, read in the browser from
  `GET /repos/zaewc/Lumen/releases/latest`. The visitor's platform gets the primary
  button (`_universal.dmg`, `_x64-setup.exe`, `_amd64.deb`/`_amd64.AppImage` by name
  suffix); every file, `SHA256SUMS` and the `gh attestation verify` command are listed
  below it, with the steps for opening an unsigned build. Draft releases and
  pre-releases are not "Latest", so publish a beta as a normal release or the site keeps
  saying that the first beta is coming. Without JavaScript, or when the API cannot be
  reached (it allows 60 unauthenticated requests per hour per IP address), every
  download link goes to the release page instead.
- **How it is deployed:** [`site.yml`](../../.github/workflows/site.yml) uploads
  `apps/site` as it is and deploys it with GitHub's Pages actions on every push to
  `main` that changes `apps/site/**` or the workflow, or when run by hand. Only the
  deploy job can write to Pages.
- **Once (done 2026-10-25):** in the repository's Settings › Pages, set **Source** to
  **GitHub Actions**. Until then the deploy job fails at `configure-pages`.
- `404.html` uses absolute `/Lumen/` paths, because Pages serves it for any missing
  path; a custom domain would need them changed.
