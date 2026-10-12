# ADR-0028: Request PRIVATESIZE in the macOS bulk pass, not a second pass

- Status: Accepted
- Date: 2026-10-12

## Context and problem statement

[ADR-0017](0017-scan-engine-and-size-accounting.md) planned to request
`ATTR_CMNEXT_PRIVATESIZE` only in a second pass, for items that may share blocks
(`EF_MAY_SHARE_BLOCKS`). The cost of `PRIVATESIZE` was unverified (a third-party
"≈5× slower" claim), and so was whether that flag catches every file whose private
size is below its allocated size.

Measured on 2026-10-12 (macOS 26, APFS, arm64, warm cache, one thread, C probe over
`~/Library`: 1.30 M files, 126 k directories, 148.0 GB allocated, 140.4 GB private):

| Pass | Time |
| --- | --- |
| `getattrlistbulk` without `PRIVATESIZE` | 39 s |
| `getattrlistbulk` with `PRIVATESIZE` on every entry | 78 s |
| without, plus `getattrlistat(PRIVATESIZE)` for gated files (569 k calls) | 65 s |

The `EF_MAY_SHARE_BLOCKS` gate missed 2,019 files (2.08 GB) whose private size was
below their allocated size:

- decmpfs-compressed files (`UF_COMPRESSED`) report a private size of 0;
- sparse files (`EF_IS_SPARSE`) differ slightly;
- files with a resource fork (Finder `Icon\r`, aliases) report only the data fork.

Widening the gate to `EF_MAY_SHARE_BLOCKS | EF_SHARES_ALL_BLOCKS | EF_IS_SPARSE |
UF_COMPRESSED` left 74 files (4 MB), all resource-fork files. No file without the
gate had a private size above its allocated size.

## Decision drivers

- Never over-promise reclaimable space (ADR-0017).
- Few system calls and little code in the `unsafe`-adjacent adapter.
- Scan time.

## Considered options

1. Second pass gated on `EF_MAY_SHARE_BLOCKS` (ADR-0017 as written).
2. Second pass with the widened gate.
3. `PRIVATESIZE` in the bulk pass for every entry, with a switch to turn it off.

## Decision outcome

Chosen option: **3**. It is exact for every file in one system-call stream, and costs
only 13 s more than option 2 on 1.3 M files, before parallelism. Option 1 is unsafe:
it would treat 2 GB as freeable that `PRIVATESIZE` does not confirm. Option 2 saves
17% for 569 k extra path lookups and a gate that must track every new way APFS
shares or hides blocks.

The switch turns `PRIVATESIZE` off for quick scans. Files then have no private size,
and the domain counts them as freeing nothing exact (`SizeFacts::private` is `None`,
`may_share_blocks` is true).

### Consequences

- Good: "reclaimable now" is `min(private, allocated)` for every file, so it is never
  above what APFS says deletion frees.
- Good: one code path, no per-file lookups.
- Bad: full scans take up to about twice as long in `getattrlistbulk` system time.
  Benchmarks (roadmap 5.19) track this.
- Bad: `PRIVATESIZE` under-counts compressed and resource-fork files (it reports 0 or
  only the data fork), so those estimates are low. That is the safe direction; a
  fork-aware estimate can be added later with its own evidence.

## More information

- [getattrlist(2)](https://keith.github.io/xcode-man-pages/getattrlist.2.html):
  `ATTR_CMNEXT_PRIVATESIZE`, `ATTR_CMNEXT_EXT_FLAGS`.
- [`docs/research/03-macos-platform.md`](../research/03-macos-platform.md) §4.
- Amends [ADR-0017](0017-scan-engine-and-size-accounting.md) (macOS enumerator only).
