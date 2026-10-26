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
3. The workflow builds each target with Lumen's built-in Jeb key (below), writes the
   SBOMs and checksums, attests them and creates a **draft** release with generated
   notes. A build fails if the `release` environment has no `JEB_API_KEY` secret.
4. Review the draft (files, checksums, notes), then publish it.

The build jobs can read the repository and nothing else (on tag runs, the two compile
steps also get the built-in Jeb key); only the attest job can mint
a signing token, and only the release job can write releases. Neither runs repository
code. Release builds use no cache. On Linux, the CLI build and SBOM jobs may reach only the
toolchain, crates.io, npm, GitHub and artifact storage; the desktop job audits its
network use while its package mirrors are pinned down. The macOS and Windows runners
cannot enforce that limit.

A pull request that changes the workflow runs the builds as a dry run; nothing is
attested or published, and no secret is used.

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
2. Create the environment and let only release tags use it:

   ```sh
   gh api -X PUT repos/zaewc/Lumen/environments/release \
     -F 'deployment_branch_policy[protected_branches]=false' \
     -F 'deployment_branch_policy[custom_branch_policies]=true'
   gh api -X POST repos/zaewc/Lumen/environments/release/deployment-branch-policies \
     -f name='v*' -f type=tag
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
  steps for opening an unsigned build. Draft releases and
  pre-releases are not "Latest", so publish a beta as a normal release or the site keeps
  saying that no public release is available. Without JavaScript, or when the API cannot be
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
