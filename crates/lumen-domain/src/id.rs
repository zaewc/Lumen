//! Typed identifiers.
//!
//! Identifiers are newtypes so that, for example, a [`ScanId`] can never be passed
//! where a [`PlanId`] is expected. The domain never *generates* identifiers: random
//! or time-based values come from an `IdGenerator` port in the application layer,
//! which keeps this crate deterministic. The domain only wraps and validates them.
//!
//! JSON encoding follows ADR-0011: UUIDs as hyphenated strings, 128-bit file IDs as
//! decimal strings (JSON numbers lose precision above 2^53).

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use uuid::Uuid;

/// Error returned when an identifier fails validation.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum IdError {
    /// The nil UUID (all zeros) is never a valid identifier.
    #[error("{kind} must not be the nil UUID")]
    Nil {
        /// Name of the identifier type.
        kind: &'static str,
    },
    /// The text is not a valid UUID.
    #[error("{kind} is not a valid UUID: {reason}")]
    InvalidUuid {
        /// Name of the identifier type.
        kind: &'static str,
        /// Parser message.
        reason: String,
    },
    /// A volume identifier was empty.
    #[error("volume identifier must not be empty")]
    EmptyVolumeId,
    /// A volume identifier exceeded [`VolumeId::MAX_LEN`] bytes.
    #[error("volume identifier is {len} bytes; the maximum is {max}", max = VolumeId::MAX_LEN)]
    VolumeIdTooLong {
        /// Actual length in bytes.
        len: usize,
    },
    /// A volume identifier contained a byte outside visible ASCII.
    #[error("volume identifier contains a character outside visible ASCII at byte {index}")]
    VolumeIdInvalidChar {
        /// Byte offset of the first invalid character.
        index: usize,
    },
    /// A file ID string was not a decimal `u128`.
    #[error("file ID must be a decimal unsigned 128-bit integer")]
    InvalidFileId,
}

macro_rules! uuid_id {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(Uuid);

        impl $name {
            /// Wraps a UUID produced by an identifier generator.
            ///
            /// # Errors
            ///
            /// Returns [`IdError::Nil`] for the nil UUID.
            pub fn new(uuid: Uuid) -> Result<Self, IdError> {
                if uuid.is_nil() {
                    Err(IdError::Nil { kind: stringify!($name) })
                } else {
                    Ok(Self(uuid))
                }
            }

            /// The underlying UUID.
            pub const fn as_uuid(&self) -> &Uuid {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                fmt::Display::fmt(&self.0.hyphenated(), f)
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}({})", stringify!($name), self.0.hyphenated())
            }
        }

        impl FromStr for $name {
            type Err = IdError;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                let uuid = Uuid::try_parse(s).map_err(|e| IdError::InvalidUuid {
                    kind: stringify!($name),
                    reason: e.to_string(),
                })?;
                Self::new(uuid)
            }
        }

        impl Serialize for $name {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.collect_str(&self.0.hyphenated())
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                let text = <std::borrow::Cow<'de, str>>::deserialize(deserializer)?;
                text.parse().map_err(serde::de::Error::custom)
            }
        }
    };
}

uuid_id! {
    /// Identifies one Lumen installation on one device. Random, created at install.
    DeviceId
}

uuid_id! {
    /// Identifies one scan run.
    ScanId
}

uuid_id! {
    /// Identifies one persisted scan snapshot (used for history and forecasting).
    SnapshotId
}

uuid_id! {
    /// Identifies one cleanup plan proposed to the user.
    PlanId
}

uuid_id! {
    /// Identifies one executed operation recorded in the ledger (e.g. a quarantine move).
    OperationId
}

/// Identifies a filesystem volume, as reported by the platform.
///
/// The value is opaque: an APFS volume UUID on macOS, a volume serial number on
/// Windows, a storage-volume name on Android. It is restricted to 1–128 bytes of
/// visible ASCII so it is safe to log, display and use as a key.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct VolumeId(String);

impl VolumeId {
    /// Maximum length in bytes.
    pub const MAX_LEN: usize = 128;

    /// Validates and wraps a platform volume identifier.
    ///
    /// # Errors
    ///
    /// Returns an [`IdError`] if the value is empty, longer than
    /// [`VolumeId::MAX_LEN`], or contains anything other than visible ASCII.
    pub fn new(value: impl Into<String>) -> Result<Self, IdError> {
        let value = value.into();
        if value.is_empty() {
            return Err(IdError::EmptyVolumeId);
        }
        if value.len() > Self::MAX_LEN {
            return Err(IdError::VolumeIdTooLong { len: value.len() });
        }
        if let Some(index) = value.bytes().position(|b| !b.is_ascii_graphic()) {
            return Err(IdError::VolumeIdInvalidChar { index });
        }
        Ok(Self(value))
    }

    /// The identifier text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for VolumeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl fmt::Debug for VolumeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "VolumeId({})", self.0)
    }
}

impl FromStr for VolumeId {
    type Err = IdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new(s)
    }
}

impl Serialize for VolumeId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for VolumeId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Self::new(text).map_err(serde::de::Error::custom)
    }
}

/// A file's identifier within its volume: the inode number on Unix-like systems or
/// the 128-bit NTFS/ReFS file ID on Windows.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FileId(u128);

impl FileId {
    /// Wraps a platform file identifier.
    pub const fn new(value: u128) -> Self {
        Self(value)
    }

    /// The raw value.
    pub const fn get(self) -> u128 {
        self.0
    }
}

impl fmt::Display for FileId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

impl fmt::Debug for FileId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "FileId({})", self.0)
    }
}

impl FromStr for FileId {
    type Err = IdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // `u128::from_str` accepts a leading '+'; the canonical form does not.
        if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
            return Err(IdError::InvalidFileId);
        }
        s.parse::<u128>()
            .map(Self)
            .map_err(|_| IdError::InvalidFileId)
    }
}

impl Serialize for FileId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for FileId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = <std::borrow::Cow<'de, str>>::deserialize(deserializer)?;
        text.parse().map_err(serde::de::Error::custom)
    }
}

/// The identity of a filesystem object: which volume, and which file on it.
///
/// Lumen keys every decision on identity rather than path text (ADR-0013,
/// ADR-0016), because paths can be swapped, normalised differently, or hard-linked.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct FileIdentity {
    /// Volume holding the file.
    pub volume: VolumeId,
    /// File identifier within the volume.
    pub file: FileId,
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    const SAMPLE: &str = "0190f1c2-7a3b-7c4d-8e5f-6a7b8c9d0e1f";

    #[test]
    fn uuid_ids_reject_nil() {
        assert_eq!(
            ScanId::new(Uuid::nil()),
            Err(IdError::Nil { kind: "ScanId" })
        );
        assert!(matches!(
            "00000000-0000-0000-0000-000000000000".parse::<PlanId>(),
            Err(IdError::Nil { kind: "PlanId" })
        ));
    }

    #[test]
    fn uuid_ids_display_hyphenated_and_debug_named() -> Result<(), IdError> {
        let id: OperationId = SAMPLE.parse()?;
        assert_eq!(id.to_string(), SAMPLE);
        assert_eq!(format!("{id:?}"), format!("OperationId({SAMPLE})"));
        Ok(())
    }

    #[test]
    fn uuid_ids_reject_malformed_text() {
        assert!(matches!(
            "not-a-uuid".parse::<DeviceId>(),
            Err(IdError::InvalidUuid {
                kind: "DeviceId",
                ..
            })
        ));
    }

    #[test]
    fn uuid_ids_serialize_as_strings_and_reject_nil_on_deserialize() -> serde_json::Result<()> {
        let json = format!("\"{SAMPLE}\"");
        let id: SnapshotId = serde_json::from_str(&json)?;
        assert_eq!(serde_json::to_string(&id)?, json);
        assert!(
            serde_json::from_str::<SnapshotId>("\"00000000-0000-0000-0000-000000000000\"").is_err()
        );
        Ok(())
    }

    #[test]
    fn volume_id_validation() {
        assert!(VolumeId::new("A1B2C3D4-0000-4000-8000-123456789ABC").is_ok());
        assert!(VolumeId::new("0x1234ABCD").is_ok());
        assert_eq!(VolumeId::new(""), Err(IdError::EmptyVolumeId));
        assert_eq!(
            VolumeId::new("a".repeat(VolumeId::MAX_LEN + 1)),
            Err(IdError::VolumeIdTooLong {
                len: VolumeId::MAX_LEN + 1
            })
        );
        assert_eq!(
            VolumeId::new("ab cd"),
            Err(IdError::VolumeIdInvalidChar { index: 2 })
        );
        assert_eq!(
            VolumeId::new("ab\ncd"),
            Err(IdError::VolumeIdInvalidChar { index: 2 })
        );
        assert_eq!(
            VolumeId::new("vol\u{202e}x"),
            Err(IdError::VolumeIdInvalidChar { index: 3 })
        );
    }

    #[test]
    fn volume_id_deserialize_validates() {
        assert!(serde_json::from_str::<VolumeId>("\"\"").is_err());
        assert!(serde_json::from_str::<VolumeId>("\"has space\"").is_err());
    }

    #[test]
    fn file_id_encodes_as_decimal_string_beyond_f64_precision() -> serde_json::Result<()> {
        let id = FileId::new(u128::MAX);
        let json = serde_json::to_string(&id)?;
        assert_eq!(json, format!("\"{}\"", u128::MAX));
        assert_eq!(serde_json::from_str::<FileId>(&json)?, id);
        Ok(())
    }

    #[test]
    fn file_id_rejects_non_canonical_text() {
        for bad in ["", "+1", "-1", " 1", "1 ", "0x10", "1e3"] {
            assert_eq!(
                bad.parse::<FileId>(),
                Err(IdError::InvalidFileId),
                "{bad:?}"
            );
        }
        assert!(
            serde_json::from_str::<FileId>("42").is_err(),
            "numbers are not accepted"
        );
    }

    #[test]
    fn file_identity_json_shape() -> Result<(), Box<dyn std::error::Error>> {
        let identity = FileIdentity {
            volume: VolumeId::new("vol-1")?,
            file: FileId::new(7),
        };
        let json = serde_json::to_string(&identity)?;
        assert_eq!(json, r#"{"volume":"vol-1","file":"7"}"#);
        assert_eq!(serde_json::from_str::<FileIdentity>(&json)?, identity);
        Ok(())
    }

    proptest! {
        #[test]
        fn uuid_id_round_trips(bits in any::<u128>().prop_filter("non-nil", |b| *b != 0)) {
            let id = ScanId::new(Uuid::from_u128(bits)).map_err(|e| TestCaseError::fail(e.to_string()))?;
            let parsed: ScanId = id.to_string().parse().map_err(|e: IdError| TestCaseError::fail(e.to_string()))?;
            prop_assert_eq!(parsed, id);
            let json = serde_json::to_string(&id).map_err(|e| TestCaseError::fail(e.to_string()))?;
            let back: ScanId = serde_json::from_str(&json).map_err(|e| TestCaseError::fail(e.to_string()))?;
            prop_assert_eq!(back, id);
        }

        #[test]
        fn file_id_round_trips(value in any::<u128>()) {
            let id = FileId::new(value);
            let parsed: FileId = id.to_string().parse().map_err(|e: IdError| TestCaseError::fail(e.to_string()))?;
            prop_assert_eq!(parsed, id);
        }

        #[test]
        fn volume_id_accepts_exactly_visible_ascii(s in "\\PC{0,140}") {
            let expected_ok = !s.is_empty()
                && s.len() <= VolumeId::MAX_LEN
                && s.bytes().all(|b| b.is_ascii_graphic());
            prop_assert_eq!(VolumeId::new(s.clone()).is_ok(), expected_ok);
        }
    }
}
