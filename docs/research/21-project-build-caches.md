# Project build caches missing from cleanup proposals

- Date: 2026-10-29
- Question: what can Lumen learn from PureMac when permanent removal has freed little
  space, without treating every large artifact as disposable?
- Outcome: use project-aware cache locations, backed by tool documentation and Lumen's
  existing safety policy. Start with Next.js's build cache.

## Comparison with PureMac

Reviewed [PureMac at 046e5c5](https://github.com/momenbasel/PureMac/tree/046e5c5b1ef67107b9651bdf2e492d18a06458c7),
particularly `cli/Sources/puremac/Core/Purger.swift`, `Catalog.swift`,
`CategoryScanner.swift` and `PureMac/Services/ScanEngine.swift`.

PureMac explores project artifacts including `.next`, Cargo `target`, `node_modules`
and virtual environments, groups them by project and avoids nested duplicate targets.
Lumen already has project-marker matching, complete-subtree accounting and nested
candidate suppression, but shipped no project-relative knowledge-base entries. This
left potentially large build caches out of the proposal list.

Adopt the project-aware discovery idea, not blanket deletion by artifact name or age.
PureMac's broad artifact list also includes deliverables and environments that can hold
local changes. Lumen must keep its evidence, coverage, in-use and execution gates.
No PureMac source code is incorporated; implementations use the existing Lumen matcher
and official tool documentation. PureMac is MIT-licensed.

## Next.js

[Next.js's build-cache guide](https://nextjs.org/docs/app/guides/ci-build-caching)
identifies `.next/cache` as the cache reused between builds. Match that exact directory
beside a scanned regular `package.json`; never match `.next` as a whole, its `server`
or `static` output, source files or `node_modules`. A directory or symbolic link named
`package.json` is not a manifest marker. Contents are never opened.

Classify this cache as regenerable, with a rebuild taking minutes and potentially
requiring network access. Its first build after cleanup can be slower. Custom
`distDir` locations remain unknown because detecting them would require configuration
interpretation; this entry covers the documented default only.

The policy still decides whether the identified cache is eligible: an incomplete
scan, cloud placeholder, protected location, unknown in-use status or active use must
not become permission to quarantine. This entry introduces no cleanup command and
does not change the executor or any policy rule.

## Local evidence and limits

A read-only query of the owner's completed scan found 13 such cache locations, with
about 1.91 GB of allocated and recorded private bytes across 540 files. This is a
discovery measurement from a scan, not a promise of reclaimable bytes or a measurement
of net free-space increase. Current existence, complete coverage, hard links, running
processes and the policy must be checked again before proposing an action. No user
files were moved or removed during this investigation.

Cargo build outputs need a separate entry and research: the official
[Cargo build-cache guide](https://doc.rust-lang.org/cargo/reference/build-cache.html)
distinguishes final artifacts from internal caches. Do not mark the entire `target`
directory as disposable merely because it has that name.
