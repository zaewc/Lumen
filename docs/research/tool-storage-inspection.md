# Explicit developer-tool storage inventory

Verified 2026-11-05. `lumen inspect xcode [--json]` queries only Apple's
`xcrun simctl runtime list --json`. It never opens runtime image paths, scans
their mounted filesystems, or selects images to delete. Run normal `scan` and
`assess` for DerivedData, device support, archives and simulator device data.

The installed Xcode's `xcrun simctl help runtime` documents JSON on the `list`
operation. A local read-only observation confirmed `identifier`, `version`,
`build`, `state` and optional `sizeBytes` fields. Unknown additional fields are
ignored; a missing field, changed identity or malformed format fails explicitly.
Missing size is unknown. Image size is not physically reclaimable space.

The executable follows the existing user-terminal PATH trust boundary (ADR-0049).
Arguments are constants, input is closed, output is bounded to 8 KiB, and a query
is stopped after 30 seconds. A full output buffer is rejected as potentially
truncated; service failures and permission errors never become an empty list.
Queries run only on explicit request, not during scanning or assessment, and
metadata stays local. Non-macOS platforms report unsupported.

[Apple's developer support guidance](https://developer.apple.com/forums/thread/812408)
recommends managing CoreSimulator through Xcode or `simctl`, and quitting Xcode
before changing files behind it.
[Apple's runtime management instructions](https://developer.apple.com/documentation/safari-developer-tools/adding-additional-simulators)
describe Xcode Settings > Components and Window > Devices and Simulators. The
inventory prints those workflows and their documentation link. This read-only
preview grants no deletion permission and does not change KEEP/REVIEW precedence.

## Docker

Verified 2026-11-06. `lumen inspect docker [--json]` uses only the Docker CLI;
it never reads Docker's internal storage (`/var/lib/docker`, Docker Desktop's VM
disk image or volume mount points). The queries, in order, are `docker --version`,
`docker context show`, `docker context inspect <name> --format '{{json
.Endpoints.docker.Host}}'`, then `--context <name> info --format '{{json
.SecurityOptions}}'` and `--context <name> system df --format json` (twice).
`docker context show` already applies Docker's precedence (a set `DOCKER_HOST`
wins over `DOCKER_CONTEXT` and reports `default`, whose endpoint is that host;
checked locally with Docker 28.2.2), so Lumen reads no Docker environment itself
and pins `--context <name>` for every later query. Pinning keeps every answer
from the same daemon; a context name starting with `-` is rejected because it
would be parsed as an option.

[`docker system df`](https://docs.docker.com/reference/cli/docker/system/df/)
reports Images, Containers, Local Volumes and Build Cache, each with a total,
active count, size and reclaimable estimate. All four rows are required; a
missing, duplicated or unknown row, or an active count above its total, fails as
an unsupported format. The two reads must agree and the context must be unchanged
afterwards, otherwise the inventory reports that Docker changed and asks for
another run. Sizes keep Docker's own units and wording; they describe the
selected daemon (possibly remote or inside a VM), not local free space.
[`SecurityOptions`](https://docs.docker.com/engine/security/rootless/) containing
`name=rootless` is reported as rootless; absent options are unknown.

Docker's reclaimable figure counts unused resources. Stopped containers and
unused volumes can hold the only copy of data, so every category is marked for
review and no `prune` command is offered or run. A stopped daemon, a permission
error or a timeout is an error, never an empty inventory. Cleanup through
Docker's own commands with explicit confirmation remains a separate change for
#706.

## Package managers

Verified 2026-11-06. `lumen inspect packages [--json]` asks Homebrew, npm and
pnpm where their caches are: `brew --cache`, `npm config get cache` and
`pnpm store path`, each after `--version`. The knowledge base already recognizes
the default locations and holds each tool's own cleanup command (ADR-0049); the
tools themselves know a configured location the defaults miss (`HOMEBREW_CACHE`,
an `.npmrc` `cache`, pnpm's `store-dir`). Queries run in the home folder, so a
project's configuration does not leak in.

The program is found the way the cleanup command would find it: the login
shell's `PATH` first (ADR-0049). On the development machine that selected npm
11.4.1 and pnpm 10.33.0 (store `v10`), while an interactive shell with a version
manager reported npm 10.8.2 and pnpm 12.11.2 (store `v11`). Reporting the
program Lumen would actually run is the point; the difference is expected.

Each answer must be a single line without control characters, the version must
contain a digit, and the location must be absolute. Anything else, a failed or
timed-out query, or a full output buffer, makes that tool's location unknown; a
missing program is "not installed". One tool's failure never hides the others.
npm's cache is refetched when needed. pnpm's store is a shared,
content-addressable store that projects hard-link into
([pnpm store](https://pnpm.io/cli/store)); it is labelled a shared store, and
only `pnpm store prune`, which keeps packages still referenced, should clear it.
Gradle has no command that reports its caches and cleans them itself
(`self_managed` in the knowledge base), so it is not queried.
