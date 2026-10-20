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

## Policy and grouping (2026-10-19, same machine)

| Benchmark | Median time | Notes |
| --- | --- | --- |
| `policy/decide quarantine candidate` | 8.8 µs | one regenerable cache through every stage to `QUARANTINE`; about 110,000 decisions/s |
| `cluster/group 5k x 768` | 33 ms | 5,000 feature prints (1,000 originals with 4 near-copies each), all pairs on 12 cores with early exit |

Readings:

- A full assessment's cost is gathering and probing, not the policy: deciding 10,000
  candidates takes about 90 ms.
- Grouping 5,000 photos is far below the time to compute their feature prints on a
  phone (ADR-0007's spike measures both).

## CI hardware

The `bench` workflow (manual) runs the same benchmarks on GitHub's `ubuntu-24.04` and
`macos-26` runners and keeps criterion's reports as artifacts. Shared runners vary,
so compare runs of the same workflow with each other, never with the numbers above.

## Real tree: `lumen scan ~/Library`

| Measure | Before streaming aggregation (#131) | After |
| --- | --- | --- |
| Entries / directories / files | 1,456,159 / 126,720 / 1,328,179 | same tree (±60 files of live drift) |
| Wall time | 24.7 s | 17.7 s |
| CPU | 7.0 s user, 108.7 s system | 3.3 s user, 91.3 s system |
| Throughput | about 59,000 entries/s | about 82,000 entries/s |
| Peak resident memory | 1.32 GB | 201 MB |
| Coverage | incomplete: 137 directories denied by privacy protection (TCC) | same |

Both runs use `PRIVATESIZE`. Logical, allocated and reclaimable totals agree within
the live drift of `~/Library` between runs (under 0.05%).

`SizeAggregator` used to keep every file's size facts and paths until the scan
ended. It now adds a single-link file (nearly every file) to its directories'
totals as soon as it is seen, and remembers only a compact identity to ignore a
repeat sighting. Only hard-linked files are kept whole, because their
reclaimability depends on where all their links are. Most of the remaining
memory is the per-directory totals map, roughly proportional to the number of
directories.

## Whole disk, recorded and assessed (2026-10-20, same machine)

`lumen scan --db` with no path (the whole startup disk, #307), then `lumen assess`,
on `main` at #320 (identity index dropped, unreachable facts not gathered):

| Step | Time | Peak memory | Result |
| --- | --- | --- | --- |
| scan, recorded | 107 s | 745 MB | 4,863,694 entries, 580,329 directories; `index.db` 1.77 GB (about 365 B per entry) |
| assess | 36 s | 4.7 GB | 234 candidates: 25 proposed for quarantine (9.2 GB), 172 for review, 37 kept |

Storage per entry is dominated by the full path (about 240 B on this disk); a
directory table plus names would save about 30% more (roadmap 13.c). Assessment
memory grows with the number of scanned entries, because every entry is loaded to
find candidates and measure their subtrees; streaming them would make it grow with
the candidates instead (13.h). Both stay within this machine's limits; on an 8 GB
machine the assessment would swap.

## How to measure a real tree

```sh
cargo build --release -p lumen-cli
/usr/bin/time -l target/release/lumen scan ~/Library --json > /dev/null
```

Scans are read-only. Run the same command twice and keep the second (warm-cache)
result.
