# Releases

> [ADR-0026](../decisions/0026-ci-and-supply-chain-security.md): release provenance
> users can verify. Workflow: [`release.yml`](../../.github/workflows/release.yml).

## What a release contains

| File | What it is |
| --- | --- |
| `lumen-aarch64-apple-darwin.tar.gz` | the `lumen` command line for Apple silicon Macs |
| `lumen-x86_64-unknown-linux-gnu.tar.gz` | for 64-bit Linux with glibc 2.39 or later (built on Ubuntu 24.04) |
| `lumen-x86_64-pc-windows-msvc.zip` | for 64-bit Windows |
| `lumen-<target>.cdx.json` | a CycloneDX 1.5 SBOM per target: every crate compiled into that binary, from `Cargo.lock` |
| `SHA256SUMS` | SHA-256 checksums of every file above |

Each archive holds the binary, `LICENSE` and `README.md`. Every file listed in
`SHA256SUMS` carries a signed SLSA build-provenance attestation, which ties it to the
workflow run, commit and tag that built it.

The binaries are **not signed** yet: macOS Gatekeeper and Windows SmartScreen warn
about them. Signing and notarization are roadmap 12.8. Desktop bundles follow in a
separate change.

## Cutting a release

1. Set `version` in the root `Cargo.toml` to the new version, in its own pull request.
2. Once that is merged, tag the merge commit `v<version>` and push the tag. The build
   fails if the tag does not match the version.
3. The workflow builds each target, writes the SBOMs and checksums, attests them and
   creates a **draft** release with generated notes.
4. Review the draft (files, checksums, notes), then publish it.

The build jobs can read the repository and nothing else; only the attest job can mint
a signing token, and only the release job can write releases. Neither runs repository
code. Release builds use no cache. On Linux, the build and SBOM jobs may reach only the
toolchain, crates.io, GitHub and artifact storage. The macOS and Windows runners
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
