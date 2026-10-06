# Architecture

Lumen is a **modular monolith** with a **hexagonal Rust core** shared by every
platform:

```text
UI (Tauri webview · browser dashboard · Expo app)
        │  view models, plan IDs
Rust core: domain → policy / graph → application use cases + ports
        │  ports
Adapters: scanners · executor · SQLite store · platform inventory · Jev · telemetry
```

- **Deterministic policy decides.** `KEEP` / `REVIEW` / `QUARANTINE`; AI is evidence
  only.
- **Reversible cleanup.** Journaled same-volume quarantine with verification and
  rollback.
- **One destructive executor.** Handle-relative operations with identity
  re-verification.
- **Honest capability model.** Each platform declares what it can do; blind spots are
  unknown, never empty.

Read next:

- [System architecture](docs/architecture/system.md)
- [Platform architecture](docs/architecture/platforms.md)
- [Roadmap](docs/architecture/roadmap.md)
- [Architecture decision records](docs/decisions/README.md)
- [Threat model](docs/security/threat-model.md)
- [Jev architecture](docs/ai/architecture.md)
