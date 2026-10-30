# npx executable cache: separate from npm downloads

- Date: 2026-10-30
- Outcome: recognize the npx cache and offer npm's explicitly confirmed native
  cleanup command, separately from the existing npm download-cache entry.

## Evidence and scope

The official [npm 11](https://docs.npmjs.com/cli/v11/commands/npm-cache/) and
[npm 12](https://docs.npmjs.com/cli/v12/commands/npm-cache/) references distinguish
`npm cache clean` from `npm cache npx rm`. The first clears downloaded package
content; the second removes cached npm-exec/npx installations. Existing Lumen
coverage of `_cacache` therefore does not cover `_npx`.

npm's [configuration source, v11.21.1](https://github.com/npm/cli/blob/v11.21.1/workspaces/config/lib/definitions/definitions.js)
derives those two directories independently from `cache`. Recognize directories
only: `~/.npm/_npx` on macOS/Linux, `%LOCALAPPDATA%\npm-cache\_npx` on Windows,
and `_npx` below either documented spelling of the npm cache environment variable.
Never match the parent cache, `_cacache`, project `node_modules`, a child package
alone, a regular file, symlink or reparse point at the cache path.

Choose `tool_action` with `npm cache npx rm --force`, rather than direct quarantine.
The [command implementation](https://github.com/npm/cli/blob/v11.21.1/lib/commands/cache.js)
requires `--force` when removing the entire npx cache; `--dry-run` avoids removal.
The entry records that preview argument vector too. npm owns the layout and removes
its configured npx cache, without uninstalling project or global packages. Reusing
these commands later can require downloading/installing their cached dependencies
again. Do not promise a fixed amount of recoverable storage.

The host still shows the exact command, requires confirmation and executes fixed
arguments without a shell. This is irreversible native cleanup, never automatic
quarantine. No policy or runner permission changes are needed. Discovery only
reads metadata; it never executes cached programs or reads their source files.

The existing environment-target guard associates the command with the configured
folder, suppresses an old default folder's command and suppresses conflicting
upper/lower-case variable locations. Cache overrides only in `.npmrc` are not
interpreted; as with the existing npm entry, npm may clean another configured
location. No caller-supplied cache path is added to the host command API. Older
npm installations that lack this subcommand fail normally, without a fallback
to direct removal. The command was verified locally with npm 11.6.2.

## Verification

- Matcher fixtures on macOS, Windows and Linux cover default/configured roots,
  non-directories, adjacent package/download locations, and ambiguous variables.
- A real CLI scan and AI-disabled assessment of a temporary configured cache show
  separate npm/npx commands and preserve both caches and project files.
- Native npm 11.6.2 fixture experiment: explicit temporary `--cache`, empty temporary
  user/global config files, temporary working directory, `--offline`,
  `--ignore-scripts`, `--no-audit` and `--no-fund`. The dry run preserves the synthetic
  npx payload; execution removes `_npx` while preserving `_cacache`, project source
  and project `node_modules`. No real user cache is cleaned and no package is fetched.
- Existing schema, embedding and policy-class checks apply to the new entry.
