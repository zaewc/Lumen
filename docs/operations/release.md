# Releases

> [ADR-0026](../decisions/0026-ci-and-supply-chain-security.md): release provenance
> users can verify. [ADR-0054](../decisions/0054-release-on-every-push-to-main.md):
> every push to `main` releases. Workflow: [`release.yml`](../../.github/workflows/release.yml).

## What a release contains

| File | What it is |
| --- | --- |
| `lumen-aarch64-apple-darwin.tar.gz` | the `lumen` command line for Apple silicon Macs |
| `lumen-x86_64-unknown-linux-gnu.tar.gz` | for 64-bit Linux with glibc 2.39 or later (built on Ubuntu 24.04) |
| `lumen-x86_64-pc-windows-msvc.zip` | for 64-bit Windows |
| `Lumen_<version>_universal.dmg` | the desktop app for macOS 13 or later, Apple silicon and Intel |
| `Lumen_<version>_x64-setup.exe` | the desktop app's per-user Windows installer (no administrator rights needed) |
| `Lumen_<version>_amd64.deb`, `Lumen_<version>_amd64.AppImage` | the desktop app for 64-bit Linux |
| `Lumen_<version>_universal.app.tar.gz` | the macOS app archive for in-app updates (ADR-0048) |
| `*.sig`, `latest.json` | updater signatures and the version, URLs and signatures offered to installed apps |
| `lumen-<target>.cdx.json` | a CycloneDX 1.5 SBOM per target: every crate compiled into that binary, from `Cargo.lock` |
| `lumen-desktop-<target>.cdx.json` | the same for the desktop app's Rust host |
| `lumen-desktop-web.cdx.json` | the npm packages bundled into the desktop app's interface, from `pnpm-lock.yaml` |
| `SHA256SUMS` | SHA-256 checksums of every file above |

Each archive holds the binary, `LICENSE` and `README.md`. Every file listed in
`SHA256SUMS` carries a signed SLSA build-provenance attestation, which ties it to the
workflow run, commit and tag that built it.

The installers are not **code-signed** yet, so macOS Gatekeeper and Windows SmartScreen
warn about these files. Updater artifacts are signed separately (ADR-0048). The
macOS app is only ad-hoc signed, so Apple silicon will run it, but
Gatekeeper still blocks it until it is allowed in System Settings › Privacy &
Security. Signing and notarization are roadmap 12.8.

## Cutting a release

Work lands on `develop`; `main` holds what is released
([ADR-0054](../decisions/0054-release-on-every-push-to-main.md)).

1. On `develop`, set `version` in the root `Cargo.toml` and in
   `apps/desktop/src-tauri/tauri.conf.json` to the new version, in its own pull
   request (`chore(release): bump to <version>`).
2. Open a pull request from `develop` into `main` and merge it with a **merge
   commit**, so `main` keeps each change's own commit for the release notes. The
   `release-version` check fails on this pull request while the version is already
   released.
3. The push to `main` runs the workflow. Its `version` job reads the version
   (`scripts/release-version.sh`): if `v<version>` exists already, nothing is built.
   Otherwise each target is built with Lumen's built-in Jeb key (below), the updater
   files are signed, the SBOMs, checksums and `latest.json` are written and attested.
   Builds require `JEB_API_KEY` from the `release` environment; the separate `sign`
   job requires `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`
   from it, and no build ever receives them.
4. The release job then checks every file against `SHA256SUMS`, and that
   `latest.json` names the version, attached files, and signatures equal to their
   `.sig` files that each name `version:<version>` in their trusted comment (the
   updater requires it, `requireSignedVersion`). Only if all of that holds does it
   create the release with generated notes, attach the files and publish it as
   **Latest**, which creates the `v<version>` tag on the merged commit. Installed apps
   are offered the update from then on.

A release that went out wrong cannot be taken back from copies that already updated:
fix it with a newer version.

0.5.0 is the first build with in-app updates. Testing installation through the
updater requires a later published version; publishing 0.5.0 alone does not verify
that installation path.

The build jobs can read the repository and nothing else (on release runs, the two compile
steps also get the built-in Jeb key). The `sign` job alone holds the updater key: it
blocks egress, installs only the locked Tauri CLI with lifecycle scripts off, and runs
nothing else while the key is set. Only the attest job can mint
a signing token, and only the release job can write releases. Neither runs repository
code. Release builds use no cache. On Linux, the CLI build and SBOM jobs may reach only the
toolchain, crates.io, npm, GitHub and artifact storage; the desktop job audits its
network use while its package mirrors are pinned down. The macOS and Windows runners
cannot enforce that limit.

A pull request that changes the workflow runs the builds as a dry run; nothing is
attested or published, and no secret is used. The `sign` job then signs with a
throwaway key, so its steps and egress allow-list are exercised too.

### Who can start a release

Anyone who can merge into `main` (set 2026-11-02, ADR-0054):

- **Branch rules** require a pull request and the status checks on both `develop` and
  `main`; they are the only gate before a release.
- **The `release` environment** accepts only `main` and has no required reviewers, so
  its secrets reach a run from `main` without anyone approving it, and never a run
  from another branch, a tag or a pull request.
- **The `release tags` ruleset** lets only repository admins move or delete `v*`
  tags. Creating one is not restricted: the release job creates it with the workflow
  token, a tag starts nothing, and a tag created early only makes the push skip that
  version.

## Rotating the updater key

Installed copies trust the public key they were built with, so a new key reaches
them only through an update signed by the old one. Rotate in two releases:

1. Generate the new key pair offline (`pnpm --filter @lumen/desktop exec tauri signer
   generate -w ~/.tauri/lumen.key`), keep an offline backup of the private key and its
   password, and put the new public key in `tauri.conf.json` in its own pull request.
2. Cut the next release as usual while the `release` environment still holds the old
   private key. The `sign` job signs it with the old key, so installed copies accept
   it; once installed, they trust only the new key.
3. After publishing that release, replace both secrets with the new key:

   ```sh
   gh secret set TAURI_SIGNING_PRIVATE_KEY --env release --repo zaewc/Lumen < ~/.tauri/lumen.key
   gh secret set TAURI_SIGNING_PRIVATE_KEY_PASSWORD --env release --repo zaewc/Lumen
   ```

4. Every later release is signed with the new key. Check its `latest.json` signatures
   name the new key id (`minisign -V -P <new public key> -m <file> -x <file>.sig`).

Copies that skip the transition release cannot verify later updates and must be
reinstalled from the download site. Whoever holds the old key can still sign updates
that those copies accept, so after a suspected compromise, say on the download site
and in the release notes that people on older versions should reinstall.

The key was rotated on 2026-10-31 (old key id `31842E07C726AE3C`, new key id
`320080298D3AA6F8`): releases up to 0.8.1 put the old key in the desktop build's
environment on runners without egress control (ADR-0048). There is no evidence it was
read; the rotation removes the need to rule that out.

The `320080298D3AA6F8` key was replaced on 2026-11-01 by `9DCAA122F70AC51F` before
any release was signed with it: its password was lost, so no update can be signed that
0.8.3 accepts. 0.9.0 is the first build that trusts the new key. Copies on 0.8.3 or
earlier are offered 0.9.0 but refuse it at the signature check, so they must be
reinstalled from the download site; say so in the release notes. The new key's password
was generated with `openssl rand`, set in the `release` environment only after a test
signature with it succeeded, and kept in the owner's password manager. Generate any
future key the same way rather than typing the password at a hidden prompt.

## Built-in Jeb key

[ADR-0042](../decisions/0042-ship-a-built-in-jeb-key-in-release-builds.md): release
builds carry one Jeb key, so people can turn AI review on without their own. **Anyone
who downloads a release can read this key out of the binary and spend its quota**, and
revoking it turns AI review off in every release that carries it. Use a key that is
for nothing else.

How it flows: the secret `JEB_API_KEY` lives in the repository's `release`
environment. Only tag-triggered runs reference that environment, and only the two
compile steps (`build` › Build, `desktop` › Bundle) receive it, as the variable
`LUMEN_JEB_BUILTIN_KEY`, which `lumen_app::ai::builtin_key()` reads at compile time
with `option_env!`. Pull request runs reference no environment and pass an empty value.
Builds without the variable (local, CI checks, dry runs) have no built-in key and
behave as before. The key is never in the repository.

### Setting it up (once)

1. Confirm with the Jeb operator (the school's AI Hub) that this key may be shipped in
   public builds.
2. Create the environment and let only `main` use it:

   ```sh
   gh api -X PUT repos/zaewc/Lumen/environments/release \
     -F 'deployment_branch_policy[protected_branches]=false' \
     -F 'deployment_branch_policy[custom_branch_policies]=true'
   gh api -X POST repos/zaewc/Lumen/environments/release/deployment-branch-policies \
     -f name=main -f type=branch
   ```

3. Set the secret. The command reads the value from standard input; paste it at the
   prompt so it never lands in your shell history:

   ```sh
   gh secret set JEB_API_KEY --env release --repo zaewc/Lumen
   ```

   Set it **only** on the environment, never as a repository secret.
4. After the next release, check a downloaded build: `lumen jev key status` says
   "Built-in Jeb key: available." and never shows the key.

### Rotating or revoking it

Revoke the key on the AI Hub, set the new one with the same `gh secret set` command,
and cut a new release: the key is compiled in, so rotation means a new release.
Until people update, releases with the old key answer "unavailable" and the policy
decides without AI review; anyone who stored their own key is unaffected.

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

- **What it is:** static pages built with [Astro](https://astro.build/) in
  [`apps/site`](../../apps/site) (`@lumen/site`): Korean at `/Lumen/` and English at
  `/Lumen/en/`, each with `<html lang>`, `hreflang` alternates and a plain link to the
  other language (no automatic redirect, nothing stored), plus `404.html`. No UI
  framework: Astro components per section in `src/components`, the copy in
  `src/i18n/ko.ts` and `en.ts` (both typed as `Messages`, so the languages cannot
  drift), and one TypeScript module for the release lookup (`src/scripts`). The pages
  load nothing from other sites and have no cookies or analytics.
- **Content-Security-Policy:** set by a `<meta>` in `src/layouts/Base.astro`:
  `default-src 'none'`, scripts, styles and images from the site itself, and
  connections to `api.github.com` only. Astro is configured to inline nothing
  (`build.inlineStylesheets: "never"`, `vite.build.assetsInlineLimit: 0`), so the built
  HTML has no inline `<script>` or `<style>` and needs no hashes; the strings the script
  writes come from `<body data-strings>`. Astro's own `security.csp` is not used, as
  there is nothing inline to hash. The policy is left out of `astro dev`, whose server
  injects inline styles and scripts.
- **Checks:** `pnpm run check` builds the site, tests the release metadata boundary,
  and type-checks the TypeScript modules with TypeScript 7. Biome validates the full
  HTML/Astro templates without disabling lint rules. `astro build` compiles the
  templates; `.astro` files are not semantically type-checked by `tsc`, so reusable
  logic stays in typed `.ts` modules. See the [migration review](../research/14-astro-download-site.md).
  Astro telemetry is off; its unused optional image optimizer is not installed.
- **Run it locally:** `pnpm --filter @lumen/site dev` (live reload, no CSP), or
  `pnpm --filter @lumen/site build` then `pnpm --filter @lumen/site preview` to see the
  built pages at <http://localhost:4321/Lumen/>.
- **Page:** a single primary download, visible pre-alpha/unsigned status, one
  disclosure for other desktop builds and installation/verification help, and
  concise safety principles. Source and documentation links stay secondary. See
  the [redesign evidence](../research/15-download-page-restraint.md).
- **What it shows:** the release GitHub marks as **Latest**, read in the browser from
  `GET /repos/zaewc/Lumen/releases/latest`. The visitor's platform gets the primary
  button (`_universal.dmg`, `_x64-setup.exe`, `_amd64.deb`/`_amd64.AppImage` by name
  suffix, taking the first available format); other files, `SHA256SUMS` and the
  `gh attestation verify` command are behind the download disclosure, with the
  steps for opening an unsigned build. Each release is published as "Latest"
  (ADR-0054). Without JavaScript, or when the API cannot be
  reached (it allows 60 unauthenticated requests per hour per IP address), every
  download link goes to the release page instead.
- **How it is deployed:** [`site.yml`](../../.github/workflows/site.yml) installs the
  site's dependencies exactly as locked, builds it, uploads `apps/site/dist` and deploys
  it with GitHub's Pages actions on every push to `main` that changes `apps/site/**`,
  the lockfile, `pnpm-workspace.yaml` or the workflow, or when run by hand. Only the
  deploy job can write to Pages.
- **Once (done 2026-10-25):** in the repository's Settings › Pages, set **Source** to
  **GitHub Actions**. Until then the deploy job fails at `configure-pages`.
- Every page links its files under `/Lumen/` (`base` in `astro.config.ts`), so
  `404.html` works for any missing path; a custom domain would change `site` and
  `base` there.
