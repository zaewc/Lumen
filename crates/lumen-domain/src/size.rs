//! Size facts and reclaim estimates.
//!
//! One file has several sizes, and naive totals overstate what deleting frees
//! (ADR-0017): hard links share one inode, APFS clones and snapshots share blocks,
//! and cloud placeholders report bytes that are not on the device. Lumen therefore
//! records [`SizeFacts`] per directory entry and computes a [`ReclaimEstimate`]
//! that counts each file once and only promises bytes it can justify.

use std::collections::HashMap;
use std::fmt;
use std::num::NonZeroU32;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::FileIdentity;

/// A number of bytes. Serialized as a decimal string (ADR-0011).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ByteCount(u64);

impl ByteCount {
    /// Zero bytes.
    pub const ZERO: Self = Self(0);

    /// Wraps a byte count.
    pub const fn new(bytes: u64) -> Self {
        Self(bytes)
    }

    /// The raw value.
    pub const fn get(self) -> u64 {
        self.0
    }

    /// Adds, saturating at `u64::MAX` (unreachable for real volumes; saturation
    /// keeps totals monotone rather than wrapping).
    #[must_use]
    pub const fn saturating_add(self, other: Self) -> Self {
        Self(self.0.saturating_add(other.0))
    }
}

impl fmt::Display for ByteCount {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

impl Serialize for ByteCount {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for ByteCount {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = <std::borrow::Cow<'de, str>>::deserialize(deserializer)?;
        parse_canonical_u64(&text).map(Self).ok_or_else(|| {
            serde::de::Error::custom("byte count must be a canonical decimal u64 string")
        })
    }
}

/// APFS clone group identifier (`ATTR_CMNEXT_CLONEID`). Serialized as a decimal
/// string.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CloneId(u64);

impl CloneId {
    /// Wraps a clone identifier.
    pub const fn new(id: u64) -> Self {
        Self(id)
    }

    /// The raw value.
    pub const fn get(self) -> u64 {
        self.0
    }
}

impl Serialize for CloneId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for CloneId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = <std::borrow::Cow<'de, str>>::deserialize(deserializer)?;
        parse_canonical_u64(&text).map(Self).ok_or_else(|| {
            serde::de::Error::custom("clone ID must be a canonical decimal u64 string")
        })
    }
}

fn parse_canonical_u64(text: &str) -> Option<u64> {
    let canonical = !text.is_empty()
        && text.bytes().all(|b| b.is_ascii_digit())
        && (text == "0" || !text.starts_with('0'));
    if canonical {
        u64::from_str(text).ok()
    } else {
        None
    }
}

/// Storage properties that change what deleting a file frees.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[allow(clippy::struct_excessive_bools)] // Independent platform flags, not a state machine.
pub struct SizeFlags {
    /// Sparse file: logical size exceeds allocated size.
    pub sparse: bool,
    /// Filesystem-compressed (NTFS, APFS decmpfs, WOF).
    pub compressed: bool,
    /// Data is not present locally (macOS `SF_DATALESS`).
    pub dataless: bool,
    /// Cloud-provider placeholder (Windows `RECALL_ON_*`, sync-root items).
    /// Deleting it would propagate to the cloud; it never counts as reclaimable.
    pub cloud_placeholder: bool,
    /// Blocks may be shared with other files (APFS clones, snapshots).
    pub may_share_blocks: bool,
}

/// Size facts for one directory entry, as observed by a scanner.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SizeFacts {
    /// File identity; entries with the same identity are links to one file.
    pub identity: FileIdentity,
    /// Logical (apparent) size: the file's length.
    pub logical: ByteCount,
    /// Bytes allocated on disk.
    pub allocated: ByteCount,
    /// Bytes freed immediately if the file were deleted (APFS
    /// `ATTR_CMNEXT_PRIVATESIZE`), when known.
    pub private: Option<ByteCount>,
    /// APFS clone group, when known.
    pub clone_id: Option<CloneId>,
    /// Number of hard links to the file (at least 1).
    pub link_count: NonZeroU32,
    /// Storage properties.
    pub flags: SizeFlags,
}

impl SizeFacts {
    /// Bytes deleting the *whole file* (all of its links) would free immediately,
    /// and whether that figure is exact.
    fn whole_file_reclaim(&self) -> (ByteCount, bool) {
        if self.flags.cloud_placeholder || self.flags.dataless {
            return (ByteCount::ZERO, true);
        }
        match self.private {
            Some(private) => (private.min(self.allocated), true),
            // Shared blocks with unknown private size: promise nothing.
            None if self.flags.may_share_blocks => (ByteCount::ZERO, false),
            None => (self.allocated, true),
        }
    }
}

/// What removing a selection of entries would free.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReclaimEstimate {
    /// Sum of logical sizes, once per file.
    pub logical: ByteCount,
    /// Sum of allocated sizes, once per file.
    pub allocated: ByteCount,
    /// Bytes freed immediately, counting a file only when every one of its links
    /// is selected.
    pub reclaimable_now: ByteCount,
    /// `false` when `reclaimable_now` is a lower bound (shared blocks with unknown
    /// private size).
    pub exact: bool,
    /// Distinct files in the selection.
    pub files: u64,
    /// Files excluded from `reclaimable_now` because some of their hard links lie
    /// outside the selection.
    pub partially_linked_files: u64,
}

impl ReclaimEstimate {
    /// Estimates reclaimable space for a selection.
    ///
    /// Each item is one directory entry (one link). Two items with the same
    /// identity are two links to the same file; the caller must not pass the same
    /// entry twice.
    pub fn for_selection<'a>(entries: impl IntoIterator<Item = &'a SizeFacts>) -> Self {
        let mut by_identity: HashMap<&FileIdentity, (&SizeFacts, u32)> = HashMap::new();
        for facts in entries {
            by_identity
                .entry(&facts.identity)
                .and_modify(|(_, links)| *links = links.saturating_add(1))
                .or_insert((facts, 1));
        }

        let mut estimate = Self {
            exact: true,
            ..Self::default()
        };
        for (facts, selected_links) in by_identity.into_values() {
            estimate.files += 1;
            estimate.logical = estimate.logical.saturating_add(facts.logical);
            estimate.allocated = estimate.allocated.saturating_add(facts.allocated);
            if selected_links < facts.link_count.get() {
                estimate.partially_linked_files += 1;
                continue;
            }
            let (bytes, exact) = facts.whole_file_reclaim();
            estimate.reclaimable_now = estimate.reclaimable_now.saturating_add(bytes);
            estimate.exact &= exact;
        }
        estimate
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;
    use crate::{FileId, VolumeId};

    fn facts(file: u128, allocated: u64, links: u32) -> SizeFacts {
        SizeFacts {
            identity: FileIdentity {
                volume: VolumeId::new("vol").unwrap_or_else(|_| unreachable!()),
                file: FileId::new(file),
            },
            logical: ByteCount::new(allocated),
            allocated: ByteCount::new(allocated),
            private: None,
            clone_id: None,
            link_count: NonZeroU32::new(links).unwrap_or(NonZeroU32::MIN),
            flags: SizeFlags::default(),
        }
    }

    #[test]
    fn plain_files_sum_allocated() {
        let a = facts(1, 100, 1);
        let b = facts(2, 50, 1);
        let e = ReclaimEstimate::for_selection([&a, &b]);
        assert_eq!(e.reclaimable_now, ByteCount::new(150));
        assert_eq!((e.files, e.partially_linked_files, e.exact), (2, 0, true));
    }

    #[test]
    fn hard_links_count_once_and_only_when_all_links_selected() {
        let link1 = facts(7, 100, 2);
        let link2 = link1.clone();
        let one = ReclaimEstimate::for_selection([&link1]);
        assert_eq!(
            (one.reclaimable_now, one.partially_linked_files),
            (ByteCount::ZERO, 1)
        );
        assert_eq!(one.allocated, ByteCount::new(100));
        let both = ReclaimEstimate::for_selection([&link1, &link2]);
        assert_eq!(
            (both.reclaimable_now, both.allocated, both.files),
            (ByteCount::new(100), ByteCount::new(100), 1)
        );
    }

    #[test]
    fn private_size_wins_and_is_capped_by_allocated() {
        let mut clone = facts(1, 1000, 1);
        clone.flags.may_share_blocks = true;
        clone.private = Some(ByteCount::new(40));
        assert_eq!(
            ReclaimEstimate::for_selection([&clone]).reclaimable_now,
            ByteCount::new(40)
        );
        clone.private = Some(ByteCount::new(5000));
        assert_eq!(
            ReclaimEstimate::for_selection([&clone]).reclaimable_now,
            ByteCount::new(1000)
        );
    }

    #[test]
    fn shared_blocks_with_unknown_private_size_promise_nothing() {
        let mut clone = facts(1, 1000, 1);
        clone.flags.may_share_blocks = true;
        let e = ReclaimEstimate::for_selection([&clone]);
        assert_eq!((e.reclaimable_now, e.exact), (ByteCount::ZERO, false));
    }

    #[test]
    fn cloud_and_dataless_files_free_nothing() {
        let mut placeholder = facts(1, 500, 1);
        placeholder.flags.cloud_placeholder = true;
        let mut dataless = facts(2, 500, 1);
        dataless.flags.dataless = true;
        let e = ReclaimEstimate::for_selection([&placeholder, &dataless]);
        assert_eq!((e.reclaimable_now, e.exact), (ByteCount::ZERO, true));
    }

    #[test]
    fn byte_counts_serialize_as_strings() -> serde_json::Result<()> {
        assert_eq!(
            serde_json::to_string(&ByteCount::new(u64::MAX))?,
            format!("\"{}\"", u64::MAX)
        );
        assert!(serde_json::from_str::<ByteCount>("12").is_err());
        assert!(serde_json::from_str::<ByteCount>("\"012\"").is_err());
        assert!(serde_json::from_str::<CloneId>("\"-1\"").is_err());
        Ok(())
    }

    fn arb_facts() -> impl Strategy<Value = SizeFacts> {
        (
            0u128..20,
            0u64..1_000_000,
            1u32..4,
            any::<[bool; 5]>(),
            prop::option::of(0u64..2_000_000),
        )
            .prop_map(|(file, allocated, links, f, private)| {
                let mut s = facts(file, allocated, links);
                s.private = private.map(ByteCount::new);
                s.flags = SizeFlags {
                    sparse: f[0],
                    compressed: f[1],
                    dataless: f[2],
                    cloud_placeholder: f[3],
                    may_share_blocks: f[4],
                };
                s
            })
    }

    /// Keeps one facts record per identity (scanners report consistent facts for
    /// all links of a file) while allowing several links per identity.
    fn consistent(mut entries: Vec<SizeFacts>) -> Vec<SizeFacts> {
        let mut first: HashMap<FileIdentity, SizeFacts> = HashMap::new();
        for e in &mut entries {
            *e = first
                .entry(e.identity.clone())
                .or_insert_with(|| e.clone())
                .clone();
        }
        entries
    }

    proptest! {
        #[test]
        fn reclaimable_never_exceeds_allocated(entries in prop::collection::vec(arb_facts(), 0..30)) {
            let entries = consistent(entries);
            let e = ReclaimEstimate::for_selection(&entries);
            prop_assert!(e.reclaimable_now <= e.allocated);
        }

        #[test]
        fn order_does_not_matter(entries in prop::collection::vec(arb_facts(), 0..30)) {
            let entries = consistent(entries);
            let mut reversed = entries.clone();
            reversed.reverse();
            prop_assert_eq!(ReclaimEstimate::for_selection(&entries), ReclaimEstimate::for_selection(&reversed));
        }

        #[test]
        fn adding_entries_never_decreases_reclaimable(
            entries in prop::collection::vec(arb_facts(), 0..30),
            split in 0usize..30,
        ) {
            let entries = consistent(entries);
            let split = split.min(entries.len());
            let partial = ReclaimEstimate::for_selection(&entries[..split]);
            let full = ReclaimEstimate::for_selection(&entries);
            prop_assert!(full.reclaimable_now >= partial.reclaimable_now);
            prop_assert!(full.allocated >= partial.allocated);
        }

        #[test]
        fn byte_count_round_trips(n: u64) {
            let json = serde_json::to_string(&ByteCount::new(n)).map_err(|e| TestCaseError::fail(e.to_string()))?;
            let back: ByteCount = serde_json::from_str(&json).map_err(|e| TestCaseError::fail(e.to_string()))?;
            prop_assert_eq!(back, ByteCount::new(n));
        }
    }
}
