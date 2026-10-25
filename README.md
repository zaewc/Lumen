# Lumen

**Storage intelligence for every device.**

Lumen analyzes a device's storage, collects evidence about what each artifact is,
who created it, and whether anything still depends on it, and then explains what
can be safely reclaimed. Cleanup is never automatic: a deterministic safety policy
decides between `KEEP`, `REVIEW`, and `QUARANTINE`, and every removal is reversible.

Target platforms: macOS, Windows, Android, iOS, and a web dashboard.

## Status

Unsigned beta builds for macOS, Windows and Linux are offered on the
[download page](https://zaewc.github.io/Lumen/) once a release is published (source in
[`apps/site`](apps/site),
[ADR-0038](docs/decisions/0038-unsigned-beta-from-a-project-website.md)).

Pre-alpha. Research (Phase 1), architecture (Phase 2), repository bootstrap
(Phase 3: Rust workspace, CI, supply-chain and security checks, hooks), the domain
model (Phase 4: [`lumen-domain`](docs/architecture/domain.md) with committed JSON
Schemas) and the scanner (Phase 5) are complete.

Lumen can **scan, read-only**, on macOS (`getattrlistbulk`, clone- and
snapshot-aware sizes, `FSEvents`), Windows (directory handles,
`ReadDirectoryChangesW`) and other Unix systems, and record results in a local
SQLite index:

```sh
cargo run -p lumen-cli -- scan ~/Library --db index.db
```

It never changes or opens the files it scans. See the
[scanner safety contract](docs/architecture/scanner.md). It cannot clean anything
yet: the evidence graph (Phase 6) is next, then policy, Jev and quarantine. See the
[roadmap](docs/architecture/roadmap.md).

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

## License

Lumen is licensed under the [Apache License, Version 2.0](LICENSE).
