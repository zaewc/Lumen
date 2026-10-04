# Lumen

**Storage intelligence for every device.**

Lumen analyzes a device's storage, collects evidence about what each artifact is,
who created it, and whether anything still depends on it, and then explains what
can be safely reclaimed. Cleanup is never automatic: a deterministic safety policy
decides between `KEEP`, `REVIEW`, and `QUARANTINE`, and every removal is reversible.

Target platforms: macOS, Windows, Android, iOS, and a web dashboard.

## Status

Pre-alpha. The project is in its research and architecture phase; no product code
exists yet. Research notes live in [`docs/research/`](docs/research/) and
architecture decisions in [`docs/decisions/`](docs/decisions/).

## Principles

- **Safety over reclaimed space.** When uncertain, Lumen keeps the file or asks.
- **Evidence, not guesses.** Every recommendation is explainable.
- **AI is advisory.** Model output is evidence; a deterministic policy decides.
- **Reversible by default.** Removal goes through quarantine with verification and rollback.
- **Local-first and private.** File contents never leave the device by default.
