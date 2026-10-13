//! Interpreting `FSEvents` (ADR-0018, ADR-0029): resume tokens and event flags.
//!
//! Pure and platform-independent, so it is tested on every CI platform; the
//! stream itself is macOS-only.

use lumen_application::ports::Change;
use lumen_domain::{RawPath, VolumeId};

// `FSEventStreamEventFlags` (`FSEvents.h`, as in `objc2-core-services` 0.3.2).
const MUST_SCAN_SUB_DIRS: u32 = 0x0000_0001;
const USER_DROPPED: u32 = 0x0000_0002;
const KERNEL_DROPPED: u32 = 0x0000_0004;
const EVENT_IDS_WRAPPED: u32 = 0x0000_0008;
const HISTORY_DONE: u32 = 0x0000_0010;
const ROOT_CHANGED: u32 = 0x0000_0020;
const MOUNT: u32 = 0x0000_0040;
const UNMOUNT: u32 = 0x0000_0080;

/// Version byte of the token encoding.
const TOKEN_VERSION: u8 = 1;
/// Version byte, 16-byte UUID, big-endian `u64` event ID.
const TOKEN_LEN: usize = 1 + 16 + 8;

/// Where an `FSEvents` feed resumes: the `FSEvents` UUID of the watched volume and the
/// last event ID whose changes were applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResumeToken {
    /// `FSEventsCopyUUIDForDevice` of the root's device.
    pub volume_uuid: [u8; 16],
    /// Last applied event ID.
    pub event_id: u64,
}

/// A token that is not one this adapter wrote.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("not an FSEvents resume token")]
pub struct BadToken;

impl ResumeToken {
    /// Encodes the token for a [`ResumePoint`](lumen_application::ports::ResumePoint).
    pub fn encode(self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(TOKEN_LEN);
        bytes.push(TOKEN_VERSION);
        bytes.extend_from_slice(&self.volume_uuid);
        bytes.extend_from_slice(&self.event_id.to_be_bytes());
        bytes
    }

    /// Decodes a token written by [`Self::encode`].
    ///
    /// # Errors
    ///
    /// Returns [`BadToken`] for any other length or version.
    pub fn decode(bytes: &[u8]) -> Result<Self, BadToken> {
        let (&[version], rest) = bytes.split_first_chunk::<1>().ok_or(BadToken)?;
        if version != TOKEN_VERSION || bytes.len() != TOKEN_LEN {
            return Err(BadToken);
        }
        let (uuid, id) = rest.split_first_chunk::<16>().ok_or(BadToken)?;
        let id: [u8; 8] = id.try_into().map_err(|_| BadToken)?;
        Ok(Self {
            volume_uuid: *uuid,
            event_id: u64::from_be_bytes(id),
        })
    }

    /// Whether replaying from this token is trustworthy on a volume whose
    /// `FSEvents` UUID is `volume_uuid` and whose current event ID is `current`. A
    /// different UUID means the volume (or its event store) was replaced; an ID
    /// beyond the current one means the history moved backwards. `FSEvents` accepts
    /// both silently, so this check is the only guard.
    pub fn is_valid_for(self, volume_uuid: [u8; 16], current: u64) -> bool {
        self.volume_uuid == volume_uuid && self.event_id <= current
    }
}

/// What one `FSEvents` event means for the index. `path` is the event's absolute
/// path (`FSEvents` reports directories with a trailing `/`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Meaning {
    /// A change to apply.
    Change(Change),
    /// The replay of history is complete; nothing to apply.
    HistoryDone,
    /// The path cannot be represented; the caller must rescan its roots.
    Unusable,
}

/// Interprets one event.
pub fn interpret(path: &[u8], flags: u32, volume: &VolumeId) -> Meaning {
    if flags & (EVENT_IDS_WRAPPED | ROOT_CHANGED) != 0 {
        return Meaning::Change(Change::RescanVolume(volume.clone()));
    }
    if flags & HISTORY_DONE != 0 {
        return Meaning::HistoryDone;
    }
    let trimmed = match path {
        [b'/'] => path,
        [rest @ .., b'/'] => rest,
        _ => path,
    };
    // FSEvents reports absolute paths; anything else cannot be placed.
    if !trimmed.starts_with(b"/") {
        return Meaning::Unusable;
    }
    let Ok(path) = RawPath::from_unix_bytes(trimmed.to_vec()) else {
        return Meaning::Unusable;
    };
    if flags & (MUST_SCAN_SUB_DIRS | USER_DROPPED | KERNEL_DROPPED | MOUNT | UNMOUNT) != 0 {
        Meaning::Change(Change::RescanSubtree(path))
    } else {
        Meaning::Change(Change::Directory(path))
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn volume() -> VolumeId {
        VolumeId::new("dev:1000010").unwrap_or_else(|_| unreachable!())
    }

    fn path(p: &str) -> RawPath {
        RawPath::from_unix_bytes(p.as_bytes().to_vec()).unwrap_or_else(|_| unreachable!())
    }

    #[test]
    fn tokens_round_trip_and_reject_foreign_bytes() {
        let token = ResumeToken {
            volume_uuid: *b"0123456789abcdef",
            event_id: 35_848_090,
        };
        let bytes = token.encode();
        assert_eq!(bytes.len(), TOKEN_LEN);
        assert_eq!(ResumeToken::decode(&bytes), Ok(token));
        assert_eq!(ResumeToken::decode(&[]), Err(BadToken));
        assert_eq!(ResumeToken::decode(&bytes[..TOKEN_LEN - 1]), Err(BadToken));
        let mut other_version = bytes.clone();
        other_version[0] = 2;
        assert_eq!(ResumeToken::decode(&other_version), Err(BadToken));
        let mut longer = bytes;
        longer.push(0);
        assert_eq!(ResumeToken::decode(&longer), Err(BadToken));
    }

    #[test]
    fn tokens_are_valid_only_for_the_same_volume_and_no_regression() {
        let token = ResumeToken {
            volume_uuid: [7; 16],
            event_id: 100,
        };
        assert!(token.is_valid_for([7; 16], 100));
        assert!(token.is_valid_for([7; 16], 5_000));
        assert!(!token.is_valid_for([8; 16], 5_000), "another volume");
        assert!(!token.is_valid_for([7; 16], 99), "history moved backwards");
    }

    #[test]
    fn ordinary_events_rescan_their_directory() {
        assert_eq!(
            interpret(b"/Users/a/probe/sub/", 0, &volume()),
            Meaning::Change(Change::Directory(path("/Users/a/probe/sub")))
        );
        assert_eq!(
            interpret(b"/", 0, &volume()),
            Meaning::Change(Change::Directory(path("/")))
        );
    }

    #[test]
    fn coalesced_and_mount_events_rescan_the_subtree() {
        for flags in [
            MUST_SCAN_SUB_DIRS,
            USER_DROPPED,
            KERNEL_DROPPED,
            MOUNT,
            UNMOUNT,
        ] {
            assert_eq!(
                interpret(b"/Users/a/", flags, &volume()),
                Meaning::Change(Change::RescanSubtree(path("/Users/a"))),
                "{flags:#x}"
            );
        }
    }

    #[test]
    fn wrapped_ids_and_root_changes_rescan_the_volume() {
        for flags in [EVENT_IDS_WRAPPED, ROOT_CHANGED, ROOT_CHANGED | HISTORY_DONE] {
            assert_eq!(
                interpret(b"/Users/a/", flags, &volume()),
                Meaning::Change(Change::RescanVolume(volume())),
                "{flags:#x}"
            );
        }
    }

    #[test]
    fn history_done_and_unusable_paths() {
        assert_eq!(
            interpret(b"/Users/a", HISTORY_DONE, &volume()),
            Meaning::HistoryDone
        );
        assert_eq!(interpret(b"", 0, &volume()), Meaning::Unusable);
        assert_eq!(interpret(b"/a\0b/", 0, &volume()), Meaning::Unusable);
        assert_eq!(interpret(b"relative/", 0, &volume()), Meaning::Unusable);
    }

    proptest! {
        #[test]
        fn decoding_never_panics(bytes in proptest::collection::vec(any::<u8>(), 0..64)) {
            let _decoded = ResumeToken::decode(&bytes);
        }

        #[test]
        fn interpreting_never_panics(
            bytes in proptest::collection::vec(any::<u8>(), 0..64),
            flags in any::<u32>(),
        ) {
            let _meaning = interpret(&bytes, flags, &volume());
        }
    }
}
