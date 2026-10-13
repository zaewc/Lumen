# Performance baselines

Measured numbers for the scanner, so regressions and improvements are visible.
Re-measure on the same machine before comparing; numbers from different machines
or cache states are not comparable.

Run the micro-benchmarks with `just bench` (criterion, `crates/*/benches/`).

## Machine

- Apple M4 Pro, 12 cores, 24 GiB, macOS 26.6.2, internal APFS SSD.
- Release build (`cargo build --release`), warm filesystem cache.
- Measured 2026-10-13.

## Micro-benchmarks

The scan benchmarks use a fixture of 100 directories × 100 files (100 bytes each),
which is 10,101 entries including the root, scanned through the real scheduler with
default settings.

| Benchmark | Median time | Throughput |
| --- | --- | --- |
| `attrbuf/parse_32k_buffer` (195 real entries) | 2.24 µs | 87 M entries/s |
| `scan/bulk_quick/10k_files` (`getattrlistbulk`, no `PRIVATESIZE`) | 6.56 ms | 1.54 M entries/s |
| `scan/std_fs/10k_files` (`std::fs`) | 13.4–14.3 ms | 0.71–0.75 M entries/s |
| `scan/bulk/10k_files` (`getattrlistbulk` with `PRIVATESIZE`) | 97.1 ms | 104 K entries/s |

Readings:

- Parsing is negligible next to the system calls.
- Without `PRIVATESIZE`, the batch enumerator is about twice as fast as `std::fs`
  (one call per 32 KiB batch instead of `readdir` plus `lstat` per entry).
- `PRIVATESIZE` costs about 9 µs per file here. That is 15× this warm fixture's
  quick pass (0.65 µs per file), but about 2× on a real, partly cold tree, where
  the quick pass itself costs more (ADR-0028: 39 s → 78 s on 1.3 M files, single
  threaded). The per-file cost, not the ratio, is the figure to track.

## Real tree: `lumen scan ~/Library`

| Measure | Value |
| --- | --- |
| Entries / directories / files | 1,456,159 / 126,720 / 1,328,179 |
| Wall time | 24.7 s (7.0 s user, 108.7 s system across workers) |
| Throughput | about 59,000 entries/s, with `PRIVATESIZE` |
| Peak resident memory | 1.32 GB |
| Coverage | incomplete: 137 directories denied by privacy protection (TCC) |

### Known issue: memory grows with the number of files

`SizeAggregator` keeps every file's size facts and every link's path until the
scan ends, so it can compute reclaimable totals for every directory, and the CLI
holds the full coverage map alongside. On 1.3 M files that is the 1.32 GB above.
That is acceptable for the developer CLI, but not for the desktop agent. Planned
fix: aggregate in the store (SQLite) by streaming per-directory sums, and keep
only hard-linked and shared files (link count above 1 or `may_share_blocks`) in
memory for the all-links-inside rule.

## How to measure a real tree

```sh
cargo build --release -p lumen-cli
/usr/bin/time -l target/release/lumen scan ~/Library --json > /dev/null
```

Scans are read-only. Run the same command twice and keep the second (warm-cache)
result.
