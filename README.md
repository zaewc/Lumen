# Lumen

**Storage intelligence for every device.**

Lumen analyzes a device's storage, collects evidence about what each artifact is,
who created it, and whether anything still depends on it, and then explains what
can be safely reclaimed. Cleanup is never automatic: a deterministic safety policy
decides between `KEEP`, `REVIEW`, and `QUARANTINE`, and every removal is reversible.

Target platforms: macOS, Windows, Android, iOS, and a web dashboard.

## Status

Pre-alpha. Research (Phase 1), architecture (Phase 2), repository bootstrap
(Phase 3: Rust workspace, CI, supply-chain and security checks, hooks) and the
domain model (Phase 4: [`lumen-domain`](docs/architecture/domain.md) with committed
JSON Schemas) are complete. Lumen cannot scan or clean anything yet; the scanner
(Phase 5) is next. See the [roadmap](docs/architecture/roadmap.md).

## Documentation

| Topic | Where |
| --- | --- |
| Architecture overview | [`ARCHITECTURE.md`](ARCHITECTURE.md), [system](docs/architecture/system.md), [platforms](docs/architecture/platforms.md) |
| Decisions | [`docs/decisions/`](docs/decisions/README.md) |
| Research | [`docs/research/`](docs/research/) |
| Security | [`SECURITY.md`](SECURITY.md), [threat model](docs/security/threat-model.md) |
| AI (Jev) | [`docs/ai/`](docs/ai/architecture.md) |
| Contributing | [`CONTRIBUTING.md`](CONTRIBUTING.md), [`AGENTS.md`](AGENTS.md) |

## Principles

- **Safety over reclaimed space.** When uncertain, Lumen keeps the file or asks.
- **Evidence, not guesses.** Every recommendation is explainable.
- **AI is advisory.** Model output is evidence; a deterministic policy decides.
- **Reversible by default.** Removal goes through quarantine with verification and rollback.
- **Local-first and private.** File contents never leave the device by default.
