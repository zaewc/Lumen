# Local recovery validation — 2026-11-08

Implementation and boundaries: [operations guide](../operations/agent-recovery.md).
Observed locally on macOS, Python 3.9.6, Codex CLI 0.164.0-alpha.11.1.
No GitHub branch push, issue creation, PR creation or merge was performed.

## Acceptance evidence

| Scenario | Reproduction and observed result |
| --- | --- |
| Agent dies immediately after editing | A real Python worker emitting Codex-compatible JSONL stages `tracked`, edits it again, creates `new`, then SIGKILLs itself before a handoff. Git index still contains `staged`; disk contains `unstaged` and `untracked`. A fresh worker/session (`session-two`) asserts all three layers and creates `continued`; supervisor records local validation. Covered by `test_fresh_session_continues_all_change_layers_after_sigkill` and `test_killed_agent_keeps_index_worktree_and_untracked`. |
| Supervisor dies just after PR creation | The GitHub test double creates PR 3, then raises simulated process death before the checkpoint receives its number. Repeating publication discovers PR 3 and makes exactly one create call in total (`test_pr_created_before_supervisor_crash_is_reused`). No real remote write. |
| Files/PR head change after tests | A check runs once and is reused at the same fingerprint. Editing a file stops for inspection, then acknowledgment runs the check again. A changed PR head, stale merge/checkout, changed base, empty required set or network failure never retains verified CI (`test_changed_files_require_inspection_and_rerun_checks`, GitHub/Actions tests, `test_network_failure_invalidates_previous_ci_evidence`). |
| Second writer, quota and repeated failure | A live second supervisor fails the canonical worktree lock. Actual supervisor SIGKILL leaves the guardian holding it until child cleanup; a separate-session child is also stopped. Structured and stderr-only usage limits enter waiting without launching another agent; same-state repeated failure stops and attempt/backoff bounds apply (process and supervisor tests). |

These recovery tests use real OS processes, signals and temporary Git repositories,
with deterministic workers instead of spending model calls to trigger failures.
Fixtures also verify caller changes survive issue-worktree preparation, check argv
does not expand shell text, private logs are 0600, and an unexpected remote SHA
causes no push/PR mutation.

## Commands and results

```sh
PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover \
  -s scripts/agent_harness -p 'test_*.py' -v
ruff check --no-cache scripts/agent_harness
CARGO_TARGET_DIR=/private/tmp/lumen-agent-recovery-harness/target/agent-harness cargo xtask lint
zizmor --offline --no-progress .github/workflows/workflow-lint.yml
git diff --check
```

Observed: 25 tests passed (6.662 seconds); Ruff, repository lint, Zizmor and whitespace
checks passed. After adding explicit private-log permissions, the affected six
process tests passed again. `actionlint` was not installed locally; the existing CI
job still runs it, followed by the recovery suite. Full `just check` was not run.

A real, unmodified `codex exec --json --sandbox workspace-write` smoke request ran
through the guardian in an empty temporary Git repository: exit 0,
`turn.completed`, session ID and turn-end usage were collected, no errors, and
process termination was confirmed in 7.129 seconds. Raw smoke logs and its private
summary are at `/private/tmp/lumen-agent-recovery-verification/` on this machine.
No intentional forced model interruption or actual account-quota exhaustion was
used; those policies are exercised deterministically by the tests above.

Read-only live GitHub observation of PR #858 showed status attached to head
`b15ec283e1a30f083b22ce2f29a8f9ae91083812`, but schemas checkout logs contained
`ea509079555880e404c19bc345633f3c3f76e2b6` (test merge). The adapter extracted that
checkout SHA. When that PR's head changed during validation, it rejected the old
run as belonging to a different head. This is observation of another PR, not CI
validation of this unpublished harness change.

The implementation remains a local dependency stack of small commits/branches.
Publish each logical unit in order only after authorization, updating the next
branch onto merged `develop` before its PR. Required CI for these changes has
not run; nothing was merged. Same-machine/POSIX, process-observation and missing
durable-file-backup limitations remain as documented in the operations guide.
