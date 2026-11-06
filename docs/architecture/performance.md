# Performance baselines

Measured numbers for the scanner, so regressions and improvements are visible.
Re-measure on the same machine before comparing; numbers from different machines
or cache states are not comparable.

Run the micro-benchmarks with `just bench` (criterion, `crates/*/benches/`).

For cleanup effectiveness, use the separate manual
[space-recovery experiment](../research/24-space-recovery-experiment.md): it records
cache estimates and observed volume readings around quarantine/finalization, with
hard-link and sparse controls. Scanner throughput is not reclaimed capacity.

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
| `cluster/group 5k x 768` | 33 ms | 5,000 feature prints (1,000 originals with 4 near-copies each), all pairs on 12 cores with early exit; superseded by the clustering section below |

Readings:

- A full assessment's cost is gathering and probing, not the policy: deciding 10,000
  candidates takes about 90 ms.
- Grouping 5,000 photos is far below the time to compute their feature prints on a
  phone (ADR-0007's spike measures both).

## Clustering (#778, 2026-11-06, same machine)

`cargo bench -p lumen-cluster` runs two benchmarks, both on deterministic vectors
(`benches/data`):
- `cluster` measures time with Criterion: 100 to 20,000 vectors; sparse (nothing
  alike), mixed (originals with four near-copies) and dense (all alike); 128, 768 and
  2,048 elements; and one worker against all 12.
- `memory` measures each call's peak heap above its input, with a counting allocator.

The threshold is 0.5 throughout. Near-copies differ by ±0.005 per element.

| Time | Median |
| --- | --- |
| mixed × 768: 100 / 1,000 / 5,000 / 10,000 / 20,000 | 0.15 / 1.8 / 30 / 146 / 657 ms |
| 5,000 × 768: sparse / mixed / dense | 29 / 29 / 265 ms |
| 5,000 mixed: 128 / 768 / 2,048 elements | 23 / 30 / 33 ms |
| one worker / all: 100 | 0.14 / 0.15 ms |
| one worker / all: 1,000 | 7.0 / 1.4 ms |
| one worker / all: 5,000 | 188 / 31 ms |

| Peak memory above the input (× 128) | Peak | Time |
| --- | --- | --- |
| sparse 20,000 | 0.6 MiB | 0.33 s |
| mixed 20,000 | 1.1 MiB | 0.35 s |
| dense 5,000 | 235 MiB | 0.10 s |
| dense 10,000 | 938 MiB | 0.42 s |
| dense 20,000 | 3,751 MiB | 2.2 s |

Readings:

- Every matching pair is kept in memory until the end. The peak therefore grows with
  the square of a dense set: 20,000 identical photos take 3.7 GiB, which a phone
  cannot spare (#779).
- At 100 vectors, all workers are slower than one: starting threads costs more than
  the comparisons (#781).
- Elements beyond 128 add little, because a pair stops summing as soon as it exceeds
  the threshold. Dense sets compare every element of every pair, so they are about
  9× slower than sparse ones at the same size (#782, #783).

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
| assess, streamed (13.h) | 25 s | 1.0 GB | identical output (`diff` of the two runs on the same database is empty) |

Storage per entry was dominated by the full path (about 240 B on this disk); see
[Scan storage](#scan-storage) for the directory table that replaced it. The assessment
first loaded every entry to find candidates and measure their subtrees (4.7–5.0 GB).
It now reads the scan in pages of 50,000 to find candidates and reads each
candidate's subtree by a primary-key range when it is decided (13.h), so memory
follows the page and the largest candidate, not the disk.

## Scan storage (2026-10-20, schema v6)

`index.db` schema v6 (roadmap 13.17) stores each directory path of a scan once, in
`dirs`, and every entry and listing as (directory, name): the path split after its
last separator (`/` for Unix, `\` for Windows paths). Directories refer to their scan
by a small integer key instead of its 36-character ID, and entries and listings
carry neither the scan nor the flavor.

Measured with the ignored test `index::size::bytes_per_entry` (200,007 synthetic
entries, 23,505 directories 9 to 15 levels deep, average path 247 B, inserted in
batches of 2,048 as the writer does):

| Layout | Bytes per entry | Of which |
| --- | --- | --- |
| v5: full path per entry and listing | 597 | entries 550, listings 46 |
| v6: directory table plus names | 217 (−64%) | entries 144, directory paths 29 plus their index 37, listings 6 |

The synthetic tree's names arrive in random order, so its pages are less full than
the real whole-disk scan's (365 B per entry with v5); the saving per entry, about
the path length less a name, is what carries over.

On the real disk (same machine, `main` at #328 against v6, run one after the other):

| Scan | v5 | v6 |
| --- | --- | --- |
| `~/Library/Developer`, `index.db` | 112 MB | 56 MB (−50%) |
| whole disk, `index.db` | 1,777 MB | 978 MB (−45%) |
| whole disk, scan time | 100 s | 71 s |
| whole disk, `assess` | 23.9 s, 1.36 GB peak | 22.3 s, 1.41 GB peak |

`assess` and `plan` outputs were identical for `~/Library/Developer`; on the whole
disk the only differences were two folders whose contents changed between the runs.

Reads keep to index seeks (checked by `EXPLAIN QUERY PLAN` in the store's tests): a
whole scan walks the directory index in path order and each directory's entries by
primary key, so it is returned by flavor, directory path, then name (`/a/bc` before
`/a/b/c`) without a sort. `entries_page` continues from the last entry's directory and
name; `entries_within` and `listings_within` read the directory's own row and then the
range of directory paths below it, so they return whole segments only.

**Upgrading rescans.** Old rows cannot be split into directories and names in SQL, so
the v6 migration drops them and marks every scan that had them (created, running,
paused, completed) `failed`: such a scan is never assessed or resumed, and the user
scans again. Scan records, snapshots (history), plans and the evidence graph are
kept. The freed pages are reused by the next scan; the file itself shrinks only on
`VACUUM`.

## Only the latest scan is kept (2026-10-20)

An assessment reads only the latest completed scan, but every scan used to stay in
`index.db`: the owner's desktop database had grown to 11.3 GB over ten scans (four
completed, five abandoned while running, one cancelled). Now a completed scan
removes the entries and listings of every earlier finished scan, and a scan left
running for six hours is marked failed and removed too (nothing resumes scans; a
younger one may belong to another process and is left alone). Scan records, root
snapshots, plans and the evidence graph stay. When that frees 256 MB or more, the
file is compacted with `VACUUM`, if the disk has room for the rewrite
(`lumen_app::space`); otherwise the free pages are reused by later scans.

## How to measure a real tree

```sh
cargo build --release -p lumen-cli
/usr/bin/time -l target/release/lumen scan ~/Library --json > /dev/null
```

Scans are read-only. Run the same command twice and keep the second (warm-cache)
result.

## Cleanup effectiveness on a labeled home

Speed alone does not show whether Lumen proposes the right things (issue #703).
`lumen_testkit::labeled` describes a small macOS home in which each path is labeled:

| Label | Meaning | Gate |
| --- | --- | --- |
| Proposed | A regenerable artifact: DerivedData, iOS DeviceSupport, Cargo `deps` and `incremental`, a Next.js cache | Must be covered by a quarantine proposal; a miss is a coverage gap |
| Review | Shown, never proposed: Xcode Archives, the npm and pip caches (tool-managed) | Proposed for quarantine is unsafe |
| Kept | User data and state: Documents, `.ssh`, Keychains, app data, build outputs, project manifests, a download | In, or containing, a proposal is unsafe |
| In use | An artifact the harness holds open while assessing | Proposed while open is unsafe |

The home also holds traps that must not change these outcomes:
- a link from an app cache to Documents;
- a hard link to a document inside the pip cache;
- an unreadable folder;
- a 1 MiB sparse file.

`crates/lumen-cli/tests/effectiveness.rs` runs the real `lumen scan` and `lumen assess --json` on it. It requires:
- no unsafe recommendations;
- every Proposed and Review path found;
- some bytes proposed.

The test runs on every macOS CI job. Without the open file, the in-use artifact is proposed and the test fails, so the trap is live.

### Seeded homes: `just effectiveness`

`cargo xtask effectiveness [--projects N] [--seed S] [--out FILE]` adds `N`
generated projects to the labeled home (`seeded_macos_home`). Each project has a
Rust project, a web project, an Xcode build and a document, with file sizes from 1
to 16 KiB drawn from the seed, so the same arguments always build the same home.

The command:
1. builds `lumen` in release mode;
2. runs `scan` twice, first and then warm, and `assess --json` with `HOME` set to
   the fixture;
3. writes JSON and a summary: timings, each run's own peak resident memory (from
   `wait4`, so the build is not counted), and the score;
4. fails on any unsafe recommendation.

The first scan is only as cold as the OS file cache allows; the tool never purges it.

Baseline on the machine above, 2026-11-06, seed 1, macOS temporary directory (APFS):

| Projects | Files | First scan | Warm scan | Assess | Peak RSS scan / assess | Proposed found | Unsafe |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 200 | 1,820 | 571 ms | 70 ms | 13.7 s | 21 / 42 MiB | 805 / 805 | 0 |
| 1,000 | 9,020 | 637 ms | 398 ms | 106.5 s | 33 / 114 MiB | 4,005 / 4,005 | 0 |

Before #766, assessing grew faster than the number of candidates: 5× the
candidates took 7.8× as long. A `sample` profile showed two costs paid once per
candidate:
- re-hashing every gathered evidence record to find the cited ones;
- re-reading the whole process table from the kernel for the in-use probe.

The evidence is now indexed once per assessment (#767). On macOS the process table
is read once per assessment and every candidate is answered from it. Quarantine and
finalize still ask the kernel live, item by item.

With both changes, same machine and seed:

| Projects | Assess before | Evidence index only | Both | Peak RSS assess | Unsafe |
| ---: | ---: | ---: | ---: | ---: | ---: |
| 200 | 13.7 s | 8.3 s | 1.5 s | 51 MiB | 0 |
| 1,000 | 106.5 s | 39.4 s | 1.7 s | 125 MiB | 0 |

These numbers are baselines, not targets; the issue rules out setting targets
before measuring.
