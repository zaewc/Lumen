# ADR-0054: Release on every push to main, developing on develop

- Status: Accepted (owner decision, 2026-11-02)
- Date: 2026-11-02
- Supersedes in part: the tag-triggered, owner-approved draft release of
  [ADR-0026](0026-ci-and-supply-chain-security.md) and
  [release.md](../operations/release.md) (release-please, roadmap 13.e, is dropped)

## Context and problem statement

A release took five manual steps: a version pull request, a `v*` tag pushed by an
admin, two approvals of the `release` environment (before the builds, before the
updater signing), and a review of the draft before publishing it. Every change
merged straight into `main`, so `main` was both where work landed and what was
released.

On 2026-11-02 the owner asked that development happen on a `develop` branch and that
a push to `main` run every release step on its own, and, asked directly, chose to
drop both the environment approvals and the manual publishing. A GitHub App to push
tags was also requested; an App can only be created in the browser, and this design
does not need one.

## Decision drivers

- One action releases: merging into `main`.
- A release still has to be exactly the version named in the source, built once,
  attested and checked before anyone can install it.
- Installed apps update from the latest published release (ADR-0048), so nothing
  wrong may be published, even briefly.
- No standing credential beyond the workflow's own token.

## Considered options

1. **Release on push to main when its version has no tag yet**; the workflow
   creates the tag when it publishes. Versions are raised on `develop` in their own
   pull request, and a check on pull requests into `main` fails while the version is
   already released.
2. **release-please on main**, as ADR-0026 planned: it opens a release pull request
   that raises the version; merging it tags and releases. Rejected: a second pull
   request per release, and tags created with the workflow token start no other
   workflow, so it needs a GitHub App or a personal token anyway.
3. **Keep tags and approvals.** Rejected by the owner.

## Decision outcome

Chosen option: **1**.

- Work branches from `develop` and merges into it (squash). A release is a pull
  request from `develop` into `main`, merged with a merge commit, after the version
  was raised on `develop` (`Cargo.toml` and `tauri.conf.json`).
- `release.yml` runs on every push to `main`. Its `version` job
  (`scripts/release-version.sh`) reads the version, requires both files to agree,
  and builds only if `v<version>` does not exist. The build, signing, SBOM and
  attestation jobs are unchanged.
- The release job checks what the owner checked by hand: every file against
  `SHA256SUMS`, and `latest.json` naming this version, attached files and their exact
  `.sig` signatures, each bound to `version:<version>`. Only then is the release
  created (as a draft, so no file is missing while it is public) and published as
  latest, which creates the tag on the pushed commit.
- `release-version.yml` fails a release pull request (`develop` into `main`) whose
  version is already released, so a release cannot be silently skipped. Any other
  pull request into `main` (a repository-setup change, say) releases nothing unless it
  raises the version itself.
- Repository settings: `develop` is the default branch; both `develop` and `main`
  require pull requests and the same status checks; the `release` environment
  accepts only `main` and has no required reviewers; the `release tags` ruleset lets
  the GitHub Actions app create `v*` tags, and still lets nobody else but admins
  create, move or delete them.

### Consequences

- Good: merging into `main` is the whole release; nothing waits for a person.
- Good: the manifest and checksum checks run on every release instead of by eye.
- Bad: the built-in Jeb key and the updater signing key are used without a person
  approving the run. Anyone who can merge into `main` can publish an update that every
  installed copy accepts; the branch rules (pull request, required checks) are the
  remaining gate. A stolen token with write access could merge and release.
- Bad: a version raised on `develop` and merged by mistake is released at once;
  undoing it means publishing a newer version, because installed copies may already
  have updated.
- Neutral: tags pushed by hand no longer start anything.

## More information

- Owner request, 2026-11-02.
- [GitHub Docs: triggering a workflow from a workflow](https://docs.github.com/en/actions/using-workflows/triggering-a-workflow#triggering-a-workflow-from-a-workflow):
  events made with `GITHUB_TOKEN` start no new workflow runs.
- [GitHub Docs: managing environments](https://docs.github.com/en/actions/managing-workflow-runs-and-deployments/managing-deployments/managing-environments-for-deployment).
