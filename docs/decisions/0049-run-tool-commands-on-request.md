# ADR-0049: Run a tool's own cleanup command when the user asks

- Status: Accepted
- Date: 2026-10-28
- Supersedes: [ADR-0035](0035-show-tool-commands-never-run-them.md) (commands are shown,
  never run); refines [ADR-0021](0021-developer-artifact-knowledge-base.md) (tool
  actions)

## Context and problem statement

Some items only their tool should clean: the pnpm store, Xcode's unavailable
simulators, the CocoaPods, npm, Go, Homebrew and uv caches, and the space inside
Docker Desktop's disk image. ADR-0035 shows each one's documented command for the user
to copy into a terminal, and Lumen never runs it. On the owner's Mac these items hold
tens of gigabytes, and copying commands into a terminal is the step people skip. The
owner asked for a button that runs the command from the app, showing the full command
first, Docker included (2026-10-28).

A tool's command is not like quarantine: it removes data at once, Lumen cannot restore
it, and Lumen cannot check by handle what it removes (ADR-0016).

## Decision drivers

- Only the reviewed knowledge base decides what can run; nothing the page, the user's
  typing or the AI says becomes a command.
- The user sees the exact command and that it cannot be undone before it runs.
- The program found is the one the user's own terminal would run, and nothing is
  passed through a shell.
- A command that removes more than caches (Docker) says what it removes.

## Considered options

1. **Run knowledge-base commands on an explicit click, as the user, without a shell**
   (the decision below).
2. **Keep showing commands only** (ADR-0035). Rejected by the owner.
3. **Run commands automatically during clean-up.** Rejected: an irreversible step
   without a click of its own.

## Decision outcome

Chosen option: **1**.

- **What can run:** an entry's `official_action` from the knowledge base, exactly as
  the knowledge base spells it (a program name and literal arguments, validated like
  the rest of the knowledge base). The page names the entry by its ID; the host looks
  the command up itself and runs it only if the latest assessment has an item that
  entry matched. The command must not wait for input: Docker's entry becomes
  `docker system prune --force`, and that is also what the page shows.
- **Finding the program:** the directories of the user's login-shell `PATH` (read by
  running the login shell with a fixed script and no other input), then the usual
  install locations; the first executable regular file named after the program, by
  absolute path. On Windows, the process's `PATH` with its executable extensions. If
  none is found, the app says the tool is not installed.
- **Running:** as the user, with the arguments passed directly (no shell; on Windows,
  `.cmd` shims are run by the standard library with its argument escaping), standard
  input closed, in the home folder, with a 15-minute limit after which it is stopped.
  The last 8 KB of its output is kept and shown. One command runs at a time.
- **In the app:** next to each command, a "Run command" button. It opens a confirmation
  with the full command, that it cannot be undone and that Lumen cannot restore what
  it removes, and for Docker what `docker system prune` removes (stopped containers,
  unused networks, dangling images and unused build cache). Afterwards the app shows
  whether it succeeded, the output, and that a new scan shows the space freed.
- **Policy:** unchanged. These items stay under "Clean up with its own command"; no
  command is part of a plan, and the AI never proposes or runs one.

### Consequences

- Good: the largest items Lumen finds on developer machines can be cleaned with one
  click, by the tool's own supported command.
- Bad: Lumen now has an irreversible step outside quarantine. It is bounded to the
  reviewed commands, an explicit confirmation and the user's own permissions.
- Bad: what a command removes is the tool's business; Lumen reports its output and the
  next scan's sizes, not a verified list.
- Neutral: a program found on `PATH` is the user's own choice of tool, as in their
  terminal.
