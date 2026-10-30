# Free-space observations across volumes

- Date: 2026-10-30
- Question: can before/after changes from different volume IDs be added together?
- Outcome: keep observations per volume; report a net change only for a single volume.

Apple's [APFS documentation](https://developer.apple.com/documentation/foundation/about-apple-file-system)
explains that volumes in one container share free space and each includes that shared
space when reporting availability. A volume ID does not prove an independent pool.
Adding equal changes from two such volumes could count the same available space twice.

The UI therefore compares the first and last readings within each volume, preserving
decreases and zero changes. With multiple volumes, it shows separate before/after
readings and explains why they are not combined. Missing or invalid coverage remains
unknown. This changes presentation only; it does not infer APFS container identities,
alter cleanup decisions, or attribute observed changes to Lumen alone.
