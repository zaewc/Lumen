# Local coding-agent recovery

This developer tool supervises Codex outside its session. It preserves the original
request and observes Git itself, including when the agent dies before a handoff.
It complements [ADR-0056](../decisions/0056-enforce-agent-rules-mechanically.md).
Requirements: Python 3.9+, Git, authenticated `gh`, and authenticated `codex` on
macOS or Linux. No Python packages or new services are needed. Windows is not
supported by this first version (`flock`, process groups and `ps` are required).

## Prepare and run

Run the CLI from a checkout containing `scripts/agent_harness`. Keep the request and
completion conditions in text files. Choose local checks for the actual change;
the `lumen-domain` command below is only an example for a domain change. Do not use
`just check` or repeat broad CI suites locally.

```sh
python3 scripts/agent_harness/cli.py init \
  --source /Users/user/Desktop/Lumen --repository zaewc/Lumen \
  --issue 123 --worktree /private/tmp/lumen-issue-123 \
  --request /private/tmp/request.txt --conditions /private/tmp/conditions.txt \
  --check 'cargo test -p lumen-domain --lib'
```

Replace `123` with a real issue. `init` reads GitHub, looks for an open PR linked by
closing-issue references, and reuses its branch/worktree. For an existing unlinked
PR or a differently named worktree branch, pass `--branch feat/existing-task`.
Exact repository, branch and `develop` base must match. Multiple matching or closed
PRs require inspection; the tool does not silently create another PR. Fork PRs and
truncated listings are rejected. New worktrees start at freshly fetched
`origin/develop`. Reused worktrees are never reset, rebased, cleaned or stashed.
`init` preserves an existing checkpoint, including its publication authorization.

`init` prints the absolute checkpoint path. Use that path for every later command:

```sh
checkpoint=/absolute/clone/.git/agent-harness/issues/123/state.json
python3 scripts/agent_harness/cli.py resume "$checkpoint" --watch
python3 scripts/agent_harness/cli.py status "$checkpoint"
```

`resume` executes one transition. `--watch` also waits for scheduled retries or
polls GitHub CI every 30 seconds, without launching an agent to wait. Ctrl-C stops
the supervisor; its guardian stops the worker and observed descendants. Run the
same command again after the guardian releases the worktree lock. A new Codex
session receives the original request/conditions, actual change summary, last
check and next step. A context-exhausted session's transcript/handoff is omitted.
For other endings, the last handoff is labeled unverified reference material.
Each Git summary is limited to 8,000 characters in that prompt; full summaries
remain in the checkpoint. Prompts travel through private stdin with `codex exec
... -`, so a large original request cannot exceed an OS argv-size limit. Command
specs retain the input privately; no shell expansion is involved.
Surrogate-escaped non-UTF-8 filenames are displayed as escapes in stdin; the
checkpoint retains the original strings and valid UTF-8 prompt text is unchanged.

Checks are trusted, operator-supplied argv (repeat `--check` for multiple checks).
There is no shell expansion. Keep commands free of credentials. Commit manually,
using repository hooks, when local work is ready; this tool does not auto-commit,
merge or release. After inspecting external edits or a manual commit:

```sh
python3 scripts/agent_harness/cli.py check "$checkpoint" --accept-changes
python3 scripts/agent_harness/cli.py resume "$checkpoint" --accept-changes \
  --next-step 'Continue after reviewing the actual worktree changes'
```

## State and recovery decisions

The common Git directory owns `agent-harness/issues/<issue>/state.json`, numbered
command specs, JSONL/stdout and stderr logs, guardian PID/result records, and
`diff.json` (actual working and staged patches). Atomic replacement and file and
directory fsync protect each checkpoint; files use mode 0600 and newly created
state directories use 0700. These records can contain source, filenames and agent
output. Keep them local and private. They are not uploaded by the supervisor.

The checkpoint includes issue, request, conditions, next step, canonical worktree,
branch/base SHA, current HEAD, separate staged/unstaged/untracked fingerprints,
commands, exit codes/signals/timings, before/after fingerprints, log paths, session
IDs, turn-end usage, attempt policy, push intent, remote/PR SHAs and CI evidence.
Git-ignored files, external dependencies, environment and tool versions are not
fingerprinted. Local check evidence applies only to the recorded Git state and
trusted unchanged environment. Submodules and special untracked files are rejected.

Each actual worktree Git directory has one kernel-backed `agent-harness.lock`.
Its guardian inherits the same lock until worker cleanup and durable results are
finished, even after supervisor SIGKILL. A second supervisor cannot write in that
worktree, including when invoked via another checkpoint. `CARGO_TARGET_DIR` is
`<worktree>/target/agent-harness`, isolated per worktree and inside its sandbox.
Do not concurrently run another Cargo process in this target directory.
The lock coordinates cooperating supervisors; manual editors and unrelated tools
must also respect the worktree's single-writer rule.

Every resume queries real Git, the exact GitHub PR and remote branch SHA. A newer
worktree is inspected from its actual patches/status, not an agent's description.
Unexpected external changes stop at `needs_inspection`; read `diff.json` and Git
before acknowledging them with `--accept-changes`.
Repeated `resume`, `check` or `publish` calls keep this gate closed until explicit
acknowledgment, even when the last observed fingerprint has already been saved.
An unfinished merge/rebase,
unmerged paths, changed branch or lost base ancestry stops at `blocked_git` and
preserves the files. Finish the Git operation yourself before resuming.

| State/reason | Action |
| --- | --- |
| `retry` / context exhausted | Fresh session after backoff; compact context, never replay the exhausted session |
| `retry` / transient error, timeout or lost supervisor | Retry after 5, 10, 20... seconds (capped at 300); default maximum 4 agent attempts |
| `waiting_usage` | Wait until structured `retry_at`/`resets_at` or `retry_after`; absent a timestamp, remain waiting without polling the agent |
| `stopped_repeated_failure` | Same failure at the same Git fingerprint twice, or attempt limit; inspect and start a newly scoped task |
| `blocked_process` | Worker termination not proven, guardian failure or missing result; no new writer is launched |
| `blocked_pr` | Saved PR no longer matches expected repository, branch and base; inspect its identity before continuing |
| `checks_failed_or_changed` | Inspect the command log and changed files, fix, then rerun `check`; do not publish |
| `local_validated` | Agent exit and required local checks recorded; task/remote validation is still separate |
| `waiting_ci` / `verified` | Current required GitHub checks pending/unproven, or verified for the recorded head and test target |

Localized CLI error prose is not a reliable reset-time API. If you know the account's
reset time, supply it explicitly (the command requires a timezone):

```sh
python3 scripts/agent_harness/cli.py resume "$checkpoint" --watch \
  --retry-at '2026-11-11T19:39:00+09:00'
```

Exit code 0 is an agent run result, not proof of task completion. A zero exit with
no `turn.completed` is treated as interrupted/unproven. Supervisor exit codes are
0 for local validation/CI observation, 2 for waiting/retry/stopped policy, and 1
for errors or failed checks. Read `status`, not only the shell exit code.

## Authorized publication and CI

Publishing requires `--allow-publish` at `init`, only for a task the owner has
explicitly authorized to publish. The agent is instructed to leave publishing to
the supervisor. This is a developer workflow boundary, not protection against a
malicious same-user agent with GitHub credentials. Existing sandbox, hooks and
repository protection remain in force. Prepare and review a PR body with Summary,
Why, Changes, Tests, Security, Risks and Follow-up, then:

```sh
python3 scripts/agent_harness/cli.py publish "$checkpoint" \
  --body-file /private/tmp/reviewed-pr-body.md --accept-changes
python3 scripts/agent_harness/cli.py resume "$checkpoint" --watch
```

The supervisor requires clean, locally checked HEAD, validates the origin push
repository, compares local/remote SHA, saves intent before pushing, and uses a
normal non-force push only to the task branch. It queries exact head repository,
branch and base before PR creation and on resume. If GitHub created the PR and the
supervisor died before recording the number, either `resume` or another `publish`
rediscovers it. Unknown publication outcomes are inspected before retrying.

CI proof comes from current `gh pr checks --required`, not a hardcoded job list.
Check records must belong to the current head or validated test-merge status SHA;
the merge target's parents must be the current head/base. Actions checks attached
to head can still test merge: execution head/base metadata and the checkout log's
`git log -1 --format=%H` output establish the actual checkout SHA separately from
the status SHA. Metadata-only jobs without a checkout explicitly record no checkout
SHA. Missing logs, stale checkouts and unsupported event types fail closed; current
`pull_request` and `push` jobs are supported. A recent `gh` with
`api --allow-escape-sequences` is needed to read colour-containing job logs.
The supervisor records head, status and actual target SHAs, rereads PR SHAs,
and rechecks the expected repository/head/base before and after collecting proof.
A disappeared exact match never falls back to a saved PR number. Verification
requires GitHub's `CLEAN` merge gate
(also accounting for required app identity/rulesets). Empty/unknown requirements,
incomplete checks or races never verify. This is conservative: unrelated review or
merge-gate blockers may also keep the task waiting. Old CI is invalidated before
remote refresh, including when the network fails. New commits require new local
checks and CI evidence. A closed/merged PR is preserved for manual inspection.

## Recovery boundary and verification

V1 supports the same machine with the worktree and Git metadata still present.
Fingerprints and patch records are **not a backup**: machine/runner loss needs a
separate durable store of the Git index (staged state), repository and uncommitted
file bytes, including untracked files. This version does not provide that store.
Do not delete a worktree or run `git clean` as part of recovery.

The guardian tracks process groups and observed descendants across new sessions,
with PID birth-time checks, then TERM/KILL and verifies termination. Deliberately
daemonizing/reparenting between observations is outside this POSIX mechanism;
use OS job containment before running tasks that spawn such services. If the
guardian itself is killed, `ps` is denied, or a spawn result is missing, recovery
fails closed. Inspect recorded PIDs and processes manually; do not remove the
lock or fabricate a successful guardian result. A newly scoped checkpoint/worktree
is required if the original process lifecycle cannot be established safely.

No live mid-turn token monitoring is claimed. `codex exec --json --sandbox
workspace-write` records JSONL, stderr, exit/signal/time; `turn.completed` usage
arrives at turn end. App Server `thread/tokenUsage/updated` is a future integration.

```sh
PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover \
  -s scripts/agent_harness -p 'test_*.py' -v
```

The tests use temporary Git repositories and fake GitHub responses: actual agent
SIGKILL before a handoff preserves all three change layers and a fresh session
continues them; supervisor SIGKILL retains the lock until descendants stop; a
separately sessioned observed child stops; PR creation followed by simulated
supervisor death creates no duplicate; changed files rerun checks; new PR heads
reject old CI; second writers, quota waits, backoff and repeated failures are
enforced. Existing `workflow-lint` runs these tests on PRs. See the
[research note](../research/33-coding-agent-harness.md) for primary sources.
