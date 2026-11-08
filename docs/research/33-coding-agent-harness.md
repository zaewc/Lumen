# Coding-agent harness

2026-11-03. How to make AI coding agents, including smaller and weaker models,
follow Lumen's operating rules reliably. Decision:
[ADR-0056](../decisions/0056-enforce-agent-rules-mechanically.md).

## Findings

**Prose is context, not enforcement.** Claude Code's memory documentation says
CLAUDE.md and AGENTS.md are treated "as context, not enforced configuration", and
that blocking an action regardless of what the model decides needs a `PreToolUse`
hook. It recommends keeping each instruction file under about 200 lines, and moving
multi-step procedures to skills and directory-specific rules to files that load only
when that part of the tree is touched.
([Claude Code: memory](https://code.claude.com/docs/en/memory))

**Nested AGENTS.md files are not read when a CLAUDE.md exists.** Claude Code reads
`AGENTS.md` only when there is no `CLAUDE.md`, `.claude/CLAUDE.md` or
`CLAUDE.local.md` in the working directory or above it. Lumen's root `CLAUDE.md`
imports `AGENTS.md`, so a plain `crates/x/AGENTS.md` would be ignored. A
subdirectory `CLAUDE.md` loads on demand, once Claude reads or edits a file in that
directory, and its `@path` imports are expanded. A `crates/x/CLAUDE.md` containing
`@AGENTS.md` therefore gives Claude the same local guide that Codex and other tools
read from `crates/x/AGENTS.md`. (Same page, "AGENTS.md" and "How CLAUDE.md files
load".)

**Hooks.** ([Claude Code: hooks](https://code.claude.com/docs/en/hooks))

- Exit code 2 blocks the action and feeds stderr to the model; exit 1 does **not**
  block. Every other non-zero code is a non-blocking error.
- `PreToolUse` input carries `tool_name` and `tool_input` (`command` for Bash;
  absolute `file_path`, plus `content` or `old_string`/`new_string` for Write and
  Edit).
- `SessionStart` plain-text stdout is added to the model's context.
- `Stop` input carries `stop_hook_active`; a `Stop` hook can keep the turn going with
  `decision: "block"` or `additionalContext`, and Claude Code stops honouring blocks
  after eight consecutive continuations.
- `$CLAUDE_PROJECT_DIR` is the project root. The `if` filter is "best effort"; the
  docs point to the permission system for hard enforcement.

**Skills and subagents.** Project skills live in `.claude/skills/<name>/SKILL.md`;
only the description (truncated at 1,536 characters) is always in context, and the
body loads when invoked; keep it under 500 lines. Project subagents live in
`.claude/agents/`; they start with a fresh context, load the same CLAUDE.md
hierarchy, and can be limited to read-only tools.
([skills](https://code.claude.com/docs/en/skills),
[subagents](https://code.claude.com/docs/en/sub-agents))

**Harness engineering.** OpenAI's account of a repository written entirely by Codex
agents (Feb 2026) keeps a short AGENTS.md (about 100 lines) as a table of contents
over a structured `docs/` that is the system of record, enforces architecture with
custom lints and structural tests whose messages "embed remediation instructions",
checks documentation freshness mechanically, and promotes recurring review feedback
from documentation into tooling. When the agent struggles, the question is "what
capability is missing, and how do we make it legible and enforceable for the agent?"
([OpenAI: harness engineering](https://openai.com/index/harness-engineering/);
summary used because the original returned 403:
[ai-playbooks review](https://ai-playbooks.pages.dev/reviews/harness-engineering-leveraging-codex-in-an-agent-first-world))

**Long-running agents.** Anthropic's harness for long-running agents has each session
make incremental progress, commit with descriptive messages and leave clear
artifacts, because agents that attempt too much at once run out of context and leave
broken, undocumented work.
([Anthropic: effective harnesses for long-running agents](https://www.anthropic.com/engineering/effective-harnesses-for-long-running-agents))
Lumen's one-change-one-PR loop already has this shape.

## Gaps found in Lumen

- `docs/architecture/system.md` §5.1 says the crate dependency rules are "checked in
  CI"; nothing checks them. Cargo prevents only cycles.
- Hard rules in `AGENTS.md` (no `--no-verify`, no push to `main`, no attribution
  trailers, no `#[cfg(target_os)]` in pure crates, generated files are regenerated,
  not edited) are prose until the commit hook or CI fails, which is late and, for
  pushes to `main`, too late: a push to `main` releases (ADR-0054).
- Safety-critical crates (`lumen-fs-exec`, `lumen-policy`, `lumen-jev`) carry their
  invariants in ADRs and crate docs, which an agent has to know to look for.
- The working loop is a list in `AGENTS.md`; an agent that skips a step (schema
  regeneration, ADR index, PR sections) finds out from CI.

Baseline on 2026-11-03: no broken relative links in the root documents or `docs/`,
every ADR listed in the index, the crate graph already follows §5.1, and no IO or
`#[cfg(target_os)]` in the pure crates. A check can be added without fixing
anything first.

## Local recovery supervisor (2026-11-08)

Implemented separately from the guard hooks; see
[usage and recovery boundaries](../operations/agent-recovery.md).
Python's standard library supplies subprocess supervision, atomic/fsynced records
and POSIX locks without a new dependency or building the Rust application. An
in-agent handoff alone loses edits when the agent dies; the chosen supervisor
reads Git and durable guardian results. App Server would support live usage, but
adds a protocol lifecycle that V1 does not need.

Official OpenAI documentation confirms `codex exec --json` emits `thread.started`,
`turn.completed` with usage, errors and item events; edits require
`--sandbox workspace-write`. `thread/tokenUsage/updated` is an App Server event,
not an exec CLI mid-turn usage feed. Checked against local CLI
`0.164.0-alpha.11.1` help and the
[non-interactive documentation](https://developers.openai.com/codex/noninteractive)
and [App Server reference](https://developers.openai.com/codex/app-server).

[GitHub CLI](https://cli.github.com/manual/gh_pr_checks) exposes current required
checks with `--required`. GitHub says checks must pass on the latest SHA and that
a test merge with status takes precedence over head-commit status; success,
skipped and neutral are accepted outcomes
([required-check troubleshooting](https://docs.github.com/en/pull-requests/how-tos/merge-and-close-pull-requests/troubleshooting-required-status-checks)).
The implementation additionally correlates concrete check records to target SHA,
verifies merge parents, rereads head/base, and requires GitHub's clean merge gate.

An actual read-only observation of PR #858 showed Actions checks attached to
`b15ec28...` while the schemas job checked out `ea50907...` (the test merge).
According to the [workflow-event reference](https://docs.github.com/en/actions/reference/workflows-and-actions/events-that-trigger-workflows),
`pull_request` uses the merge ref by default. The supervisor therefore verifies
run head/base and checkout-log SHA separately; the status attachment alone does
not attest to the code tested. Metadata-only jobs record an absent checkout SHA.
