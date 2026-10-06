# ADR-0019: Detect duplicates in stages with a cryptographic content ID

- Status: Accepted
- Date: 2026-10-06

## Context and problem statement

Hashing every byte of every file is slow and, for cloud placeholders, triggers
downloads. A hash collision that leads to removing a unique file is data loss.
Already-deduplicated files (hard links, APFS clones) look like duplicates but removing
one frees nothing.

## Decision drivers

- Read as few bytes as possible.
- Zero tolerance for collision-induced loss.
- Never hydrate placeholders; keep content hashes local (hashes of known files can
  identify content).

## Considered options

1. Staged pipeline: size → identity collapse → XXH3-128 head and tail → BLAKE3 full
   content ID, with optional byte comparison before action.
2. Full cryptographic hash of every file.
3. Fast non-cryptographic hash only.

## Decision outcome

Chosen option: **1**, following the fclones model.

1. Group by size, skipping empty files, placeholders and dataless files.
2. Collapse identical file identities: hard links and clones are "already deduplicated"
   (APFS clone-dedupe can be offered as an alternative action).
3. XXH3-128 over a head and a tail sample (4 KiB on SSD, 16 KiB on HDD; configurable).
4. BLAKE3 over the full content, cached in `hash_cache` keyed by
   `(volume_id, file_identity, size, mtime, ctime)`. The `b3:` digest is the
   content-addressable ID of the content node in the evidence graph.
5. Optional byte-for-byte comparison before any action.

Duplicate groups are evidence. Which copy to keep follows deterministic keeper rules
(e.g. prefer user-visible locations, the oldest original, or not inside a cache), and
removal still goes through the policy engine and quarantine.

Near-duplicate images (perceptual hashes via `image_hasher` on desktop; Vision feature
prints on iOS) are `REVIEW` groups only and never trigger `QUARANTINE` on their own.
Thresholds require a labelled evaluation set before shipping.

Content hashes never leave the device by default and are excluded from telemetry and
any sync.

### Consequences

- Good: most candidates are eliminated after reading a few KiB.
- Good: BLAKE3 content IDs double as evidence-graph keys.
- Bad: two hash algorithms to maintain.

## More information

- [Scanning research, Implications 9](../research/10-filesystem-scanning-quarantine-duplicates.md)
- [iOS research, Implication 5](../research/06-ios-and-mobile-framework.md)
