# Measuring estimates and observed free space

- Date: 2026-10-30
- Question: does recognized cache size translate into actual available storage?
- Scope: a repeatable, manual Unix fixture experiment; no real user cleanup.

## Protocol

Run after compilation and other disk-heavy work have finished:

```sh
cargo test -p lumen-app --test space_recovery --locked --no-run
LUMEN_RECLAIM_BENCH_MIB=64 cargo test -p lumen-app --test space_recovery --locked -- --ignored --nocapture
```

The size is **per case**, accepts 16–256 MiB and defaults to 64 MiB. Each case
owns fresh temporary source and quarantine directories. The normal 2 GB reserve
plus the payload and 32 MiB index allowance must remain available before writing.
Run three times on the same machine and filesystem; keep all JSON result lines,
including negative or zero deltas. Do not compare different machines as a speed
or space-recovery regression.

The four cases are:

1. A non-sparse Cargo `target/debug/deps` payload beside a scanned `Cargo.toml`.
2. A non-sparse Next.js `.next/cache` payload beside a scanned `package.json`.
3. A Cargo payload with another hard link outside the cache and the scanned tree.
4. A Next.js payload extended sparsely rather than filled with data.

The real scanner and assessment discover the cache, recording its estimate and
verdict with AI disabled. Separately, the real handle-based executor quarantines
and finalizes **only the synthetic fixture cache**. This second part is an explicit
storage experiment, including KEEP cases, not the app's plan/confirmation workflow
and never a change to its policy. It does not prove production eligibility. No
home directory override, user quarantine, native cleanup tool or model is used.

Assertions check complete scanning, cache discovery, no source/final-output
quarantine proposals, unchanged identity/allocation after quarantine, disappearance
after finalization, surviving source/output files, and a surviving hard link. The
hard-link and sparse cases must have a zero estimate and stay KEEP. A failure is
not silently skipped. Ordinary `just check` compiles but does not run this manual
experiment, which writes substantial data and waits for the filesystem.

Output separates logical size, allocated file bytes, estimated reclaim, the verdict,
and available space before/after fixture creation, quarantine and finalization.
Post-finalization readings are taken immediately and after 250 ms, 1 s and 3 s.
The payload handle is flushed and closed before scanning. Values are byte counts;
`after - before` is a signed observation, never clamped to zero.

## Interpretation

- Quarantine is a same-volume rename: payload identity and allocation must survive.
  It is not space recovery, even if unrelated disk activity increases availability.
- Removing one hard link keeps the payload allocated through its remaining name.
- Sparse logical bytes are not a promise of allocated or reclaimable bytes.
- APFS clones, snapshots and filesystem delays can retain blocks. The portable
  Linux scanner cannot exclude sharing, so a private-looking payload may also have
  a zero lower bound and stay KEEP. Never relax that gate to improve this result.
- Other processes and index/manifest metadata affect volume-wide readings. No
  assertion equates a free-space delta with the payload, and this fixture result
  is not a claim of gigabytes recovered from a person's real storage.

The production observation contract remains
[per-volume readings](22-free-space-observations.md). Real-machine comparisons and
Windows second-drive lifecycle validation remain separate follow-ups.

## Recorded result: 2026-10-30

macOS 26.6.2, arm64, local temporary filesystem; three sequential runs, 64 MiB
logical payload per case. Compilation finished before sampling. The complete
signed deltas and preservation counts are in the
[CSV](24-space-recovery-results.csv); `after - before` always uses that case's
pre-finalization reading, not the initial disk reading before fixture creation.

| Case | Allocated / estimated bytes | Immediate delta, run 1 | Run 2 | Run 3 |
| --- | --- | --- | --- | --- |
| Cargo private | 67,108,864 / 67,108,864 | +67,117,056 | +67,117,056 | +67,117,056 |
| Next.js private | 67,108,864 / 67,108,864 | +67,153,920 | +67,121,152 | +67,145,728 |
| Hard link outside cache | 67,108,864 / 0 | +32,768 | -49,152 | +32,768 |
| Sparse | 0 / 0 | 0 | 0 | +4,096 |

All four cases were discovered on every run. The scanner reported exact estimates;
all source/output preservation and held-payload identity/allocation assertions
passed, including the surviving hard link. The disk immediately reflected about
64 MiB after removing the private payloads; the controls stayed near zero.

Later readings show why this must not become a promise: the sparse control's
three-second delta ranged from -43,954,176 to +97,906,688 bytes despite its payload
occupying zero bytes. Other disk activity dwarfed metadata overhead. Longer waits
did not improve attribution on this machine.

The real policy returned KEEP for these temporary paths because process visibility
was partial outside the user's home. The explicit fixture executor experiment is
therefore evidence about physical removal and the size model, **not** evidence
that the production app proposed or executed these caches. No eligibility guard
was loosened. The next product measurement should use owner-confirmed caches in
the normal app workflow and record the same distinction.
