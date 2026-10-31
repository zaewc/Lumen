# Project `node_modules` beside a lockfile

- Date: 2026-10-31
- Question: [research 21](21-project-build-caches.md) left project `node_modules` out
  because it can hold local changes. When can Lumen treat it as regenerable, so that
  quarantining it and reinstalling gives back the same tree?
- Outcome: entry `node-modules-with-lockfile`, class `regenerable` (minutes, network).
  It matches only a real `node_modules` directory beside a scanned regular lockfile of
  npm, pnpm, Yarn or Bun, never one nested inside another `node_modules`.

## Sources

Read 2026-10-31.

- npm, [`npm ci`](https://docs.npmjs.com/cli/v11/commands/npm-ci): "If a
  node_modules is already present, it will be automatically removed before npm ci
  begins its install." "The project must have an existing package-lock.json or
  npm-shrinkwrap.json." "It will never write to package.json or any of the
  package-locks: installs are essentially frozen."
- npm, [`package-lock.json`](https://docs.npmjs.com/cli/v11/configuring-npm/package-lock-json):
  it "describes the exact tree that was generated, such that subsequent installs are
  able to generate identical trees". It "cannot be published, and it will be ignored
  if found in any place other than the root project"; `npm-shrinkwrap.json`, by
  contrast, "allows publication".
- pnpm, [`pnpm install`](https://pnpm.io/cli/install): with `--frozen-lockfile`
  "pnpm doesn't generate a lockfile and fails to install if the lockfile is out of
  sync with the manifest"; it is the default in CI when a lockfile is present.
- pnpm, [`packageImportMethod`](https://pnpm.io/settings/node-modules#packageimportmethod):
  the default `auto` imports files from the content-addressable store by hard link
  first on Linux and by clone first on macOS and Windows, copying only when neither
  works.
- Yarn Classic, [`yarn install`](https://classic.yarnpkg.com/en/docs/cli/install):
  "Install all the dependencies listed within `package.json` in the local
  `node_modules` folder." "If `yarn.lock` is present and is enough to satisfy all the
  dependencies listed in `package.json`, the exact versions recorded in `yarn.lock`
  are installed".
- Yarn Berry, [`yarn install`](https://yarnpkg.com/cli/install) and
  [`nodeLinker`](https://yarnpkg.com/configuration/yarnrc): `--immutable` aborts "if
  the lockfile was to be modified"; with `nodeLinker: node-modules` "a regular
  `node_modules` folder just like in Yarn Classic or npm will be created", and with
  `pnpm` one made of "symlinks and hardlinks to a global content-addressable store".
- Bun, [`bun install`](https://bun.com/docs/cli/install): it writes `bun.lock`;
  "Before Bun 1.2, the lockfile was binary and called `bun.lockb`." With
  `--frozen-lockfile`, "Bun installs the exact versions specified in the lockfile and
  does not update it." Its default backend is `clonefile` on macOS and `hardlink` on
  Linux and Windows, falling back to copying.

Each tool rebuilds `node_modules` from the lockfile and owns nothing in it that the
lockfile, the manifest and the tool's cache cannot produce again. npm documents that it
deletes the folder itself before a clean install.

## Scope

In:

- `node_modules` of directory kind directly beside a scanned regular file named
  `package-lock.json`, `npm-shrinkwrap.json`, `pnpm-lock.yaml`, `yarn.lock`,
  `bun.lock` or `bun.lockb`, on macOS, Windows and Linux. Markers are any-of, so one
  location covers every manager; a folder with two lockfiles is still pinned by each.
  Lockfile contents are never opened: the name is the evidence.

Out:

- A `node_modules` without a lockfile beside it: without one a reinstall resolves
  versions anew. This includes workspace packages' own `node_modules` in a monorepo
  whose lockfile sits at the workspace root. They stay unrecognised (false negatives,
  the safe direction).
- A `node_modules` inside another `node_modules`. Packages ship `yarn.lock` and
  `npm-shrinkwrap.json`, so a marker can sit beside a nested folder; that marker was
  published with the package and pins nothing for this project. The matcher now
  refuses any project location whose root lies inside another instance of its own
  path. The rule is generic: for Cargo and Next.js it only excludes copies nested in
  their own caches. Independently, candidate selection already decides a match inside
  another match together with the outer one (`lumen_application::assess::select`).
- A symlink, junction, reparse point or regular file named `node_modules`, and a
  lockfile that is not a regular file (directory kind only, as for Cargo).
- pnpm's store, Bun's `~/.bun/install/cache`, npm's `_cacache` and Yarn's caches:
  other entries, or none yet. Yarn Plug'n'Play (`.pnp.cjs`, `.yarn/cache`) creates no
  `node_modules` and is not matched.

## Risks

- **Local edits.** Debugging patches made directly inside `node_modules` are not in
  the lockfile. Quarantine is reversible, so they can be restored until retention ends;
  after finalization they are gone. Patches kept with `patch-package`, pnpm
  `patchedDependencies` or Yarn `patch:` live outside `node_modules` and are reapplied.
- **Drift.** A tree changed by `npm install --no-save` or a manual copy differs from
  the lockfile; a reinstall gives the lockfile's tree, not the drifted one.
- **Links.** `npm link`, `file:` and `link:` dependencies and pnpm's layout put
  symbolic links in `node_modules`. The executor moves and finalizes a link as a link,
  never its target, so the linked source folder is untouched; the link is recreated
  on reinstall.
- **Shared blocks.** pnpm and Yarn's `pnpm` linker hard-link (Linux) or clone (macOS,
  Windows) files from a store; Bun does the same. Lumen's sizes are per file identity:
  a file with a link outside the candidate is reclaimable nowhere, and a clone counts
  only its private bytes, so the estimate shows the small remainder honestly. When it
  is exactly zero, `eligible.frees-nothing` keeps the item (ADR-0039); otherwise only
  the remainder is claimed. du-style totals below overstate these folders.
- **Reinstall cost.** Install scripts run again (native builds, binary downloads such
  as Electron's), which takes minutes and usually the network; offline machines may
  not be able to reinstall until online. The first build after a reinstall is slower
  because `node_modules/.cache` is gone too.
- **Running dev servers.** The in-use probe reports processes with an open descriptor,
  working directory or executable under the folder (and, on Linux, mapped files). A
  dev server that runs a tool binary from `node_modules` (esbuild's service) or holds
  files open there keeps it. Node closes module files after loading them, so a server
  that only loaded JavaScript may not be seen; the policy's existing in-use gates are
  unchanged, and quarantine can be rolled back.

## Local measurement

Read-only on `~/Desktop`, 2026-10-31: `find -name node_modules -type d -prune`, file
type tests for lockfile names beside each folder and `du -sk`. No file was opened,
moved or removed.

| Outermost `node_modules` | Folders | `du` size |
| --- | --- | --- |
| Beside `package-lock.json` only | 28 | 20.31 GB |
| Beside `pnpm-lock.yaml` only | 4 | 2.80 GB |
| Beside both `package-lock.json` and `pnpm-lock.yaml` | 3 | 1.41 GB |
| Beside `yarn.lock` | 2 | 1.48 GB |
| No lockfile beside it (out of scope) | 135 | 0.87 GB |

Inside those folders, 78 lockfiles were shipped by packages (71 `yarn.lock`, 5
`package-lock.json`, 2 `pnpm-lock.yaml`); 12 sat beside a nested `node_modules`
directory and would have matched without the nesting rule. `du` counts a hard-linked
or cloned file in full, so the pnpm rows are an upper bound on reclaimable space, not
a measured free-space gain ([research 24](24-space-recovery-experiment.md)).
