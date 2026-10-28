# ADR-0035: Show tools' own cleanup commands; never run them

- Status: Superseded by [ADR-0049](0049-run-tool-commands-on-request.md) (commands run when the user asks)
- Date: 2026-10-20
- Amends: [ADR-0021](0021-developer-artifact-knowledge-base.md) (tool actions)

## Context and problem statement

ADR-0021 gives each knowledge-base entry an `official_action`, the tool's own cleanup
command, and plans for Lumen to run it: as the user, from known install locations,
with dry runs captured as evidence and the result labelled irreversible. None of that
exists yet. The policy keeps such items at `REVIEW` ("only the tool's own cleanup
command should remove it"), and the interface does not say which command. On the
owner's Mac, Xcode's simulator devices are 8.5 GB of `REVIEW` with a documented
command (`xcrun simctl delete unavailable`) the user never sees.

ADR-0021 also forbids an action on `not_regenerable` entries. That rule assumed the
action would be run and would remove the item. Some irreplaceable items have a vendor
command that frees space *inside* them without removing them. Docker Desktop's disk
image holds containers and images that cannot be regenerated, but Docker documents
`docker system prune` to reclaim space inside it.

How should Lumen help with items only their tool should clean, without taking a new
irreversible step itself?

## Decision drivers

- Safety: Lumen's only irreversible step is removing a quarantined item for good,
  after an in-use check (ADR-0015). A tool's command is irreversible and does things
  Lumen cannot verify by handle (ADR-0016).
- Users should not have to look up the command themselves.
- The knowledge base stays data reviewed like code, with vendor sources.

## Considered options

1. **Show the command, never run it:** the interface and the CLI display the entry's
   command for the user to copy and run themselves, with a note that Lumen does not
   run it. Irreplaceable entries may carry a command that frees space inside them.
2. **Run the command** as ADR-0021 planned: resolved binaries, dry run, timeouts,
   captured output.
3. **Keep silent** until option 2 exists.

## Decision outcome

Chosen option: **1**.

- An entry's `official_action` is guidance. Lumen shows it with the tool's name, never
  runs it, and says so next to it. The policy is unchanged: these items stay at
  `REVIEW` or `KEEP`, and nothing about them enters a plan.
- A `not_regenerable` entry may have an `official_action` when the vendor documents a
  command that frees space inside the item without removing it. Its effort stays
  `impossible`. The interface shows such a command like any other, with the note to
  check what it removes first.
- Running commands (ADR-0021's tool actions) stays future work. If it is ever built, it
  needs its own ADR covering binary resolution, dry runs, confirmation and the
  irreversible label. This ADR does not authorise it.

### Consequences

- Good: users learn the safe, vendor-supported way to reclaim space Lumen will not
  touch, such as the 8.5 GB of simulator devices on the owner's Mac.
- Good: no new irreversible code path in Lumen.
- Bad: Lumen cannot confirm the command was run or what it freed until the next scan.
- Bad: a command can remove more than the user expects (`docker system prune` removes
  stopped containers). The note asks users to check first; the entry's sources
  document the effect.

## More information

- [ADR-0021](0021-developer-artifact-knowledge-base.md): the knowledge base and its
  planned tool actions.
- Docker: [Mac FAQs, disk image and `docker system prune`](https://docs.docker.com/desktop/troubleshoot-and-support/faqs/macfaqs/).
