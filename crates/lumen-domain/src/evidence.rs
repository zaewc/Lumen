//! Immutable, content-addressed evidence.
//!
//! An [`Evidence`] record is one fact about one subject, with its provenance.
//! Records are never edited: a new observation is a new record (ADR-0013). Each
//! record is identified by the BLAKE3 hash of its canonical serialization, so the
//! evidence behind any decision can be verified and replayed later.

use std::collections::BTreeSet;
use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::{
    AppId, CodeSignature, FileIdentity, Protection, ServiceKind, SourceName, Timestamp,
    UntrustedText, VolumeId,
};

/// Domain-separation prefix for evidence hashing. Bumped whenever the canonical
/// form changes, so old and new hashes can never collide.
const HASH_DOMAIN: &[u8] = b"lumen.evidence/1\0";

/// Content address of an [`Evidence`] record: `b3:` followed by 64 lowercase hex
/// digits of BLAKE3.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EvidenceId([u8; 32]);

/// Error returned for malformed evidence identifiers.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("evidence ID must be 'b3:' followed by 64 lowercase hex digits")]
pub struct EvidenceIdError;

impl EvidenceId {
    /// The raw 32-byte digest.
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Display for EvidenceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("b3:")?;
        self.0.iter().try_for_each(|b| write!(f, "{b:02x}"))
    }
}

impl fmt::Debug for EvidenceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "EvidenceId({self})")
    }
}

impl FromStr for EvidenceId {
    type Err = EvidenceIdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let hex = s.strip_prefix("b3:").ok_or(EvidenceIdError)?;
        if hex.len() != 64 {
            return Err(EvidenceIdError);
        }
        let nibble = |b: u8| match b {
            b'0'..=b'9' => Ok(b - b'0'),
            b'a'..=b'f' => Ok(b - b'a' + 10),
            _ => Err(EvidenceIdError),
        };
        let mut out = [0u8; 32];
        for (byte, &[hi, lo]) in out.iter_mut().zip(hex.as_bytes().as_chunks::<2>().0) {
            *byte = (nibble(hi)? << 4) | nibble(lo)?;
        }
        Ok(Self(out))
    }
}

impl Serialize for EvidenceId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for EvidenceId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = <std::borrow::Cow<'de, str>>::deserialize(deserializer)?;
        text.parse().map_err(serde::de::Error::custom)
    }
}

/// What a piece of evidence is about.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[non_exhaustive]
pub enum Subject {
    /// A filesystem object.
    File {
        /// Identity of the object.
        identity: FileIdentity,
    },
    /// An installed application.
    Application {
        /// One identifier of the application.
        id: AppId,
    },
    /// A volume.
    Volume {
        /// Volume identifier.
        id: VolumeId,
    },
    /// A running process. PIDs are reused, so the start time is part of the key
    /// whenever the platform reports it.
    Process {
        /// Process ID.
        pid: u32,
        /// Process start time.
        started_at: Option<Timestamp>,
    },
    /// A background, startup or scheduled component.
    Service {
        /// Component kind.
        kind: ServiceKind,
        /// Label, service name or task name.
        label: UntrustedText,
    },
}

/// A typed fact. Each variant fixes the type of its value, so a fact can never be
/// paired with the wrong kind of value.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "fact", content = "value", rename_all = "snake_case")]
#[non_exhaustive]
pub enum Fact {
    /// The subject belongs to this application (by location convention, receipt,
    /// signature or knowledge base, see provenance).
    OwningApplication(AppId),
    /// Whether this application is installed. `false` is only meaningful with a
    /// [`Basis::Absence`] that names the searched sources.
    ApplicationPresent {
        /// The application.
        app: AppId,
        /// Whether it was found.
        present: bool,
    },
    /// Content last modified.
    LastModified(Timestamp),
    /// Last accessed (weak evidence; access times are often not maintained).
    LastAccessed(Timestamp),
    /// Whether a running process holds the subject open or depends on it.
    InUse(bool),
    /// A vendor declared the subject a disposable cache (e.g. a Windows Disk
    /// Cleanup handler registration).
    VendorDeclaredCache {
        /// Who made the declaration.
        declared_by: UntrustedText,
    },
    /// The subject matches a developer knowledge-base entry (ADR-0021).
    KnowledgeBaseMatch {
        /// Knowledge-base entry ID.
        entry: SourceName,
    },
    /// Code signature of the subject.
    Signature(CodeSignature),
    /// Whether the subject is cloud-backed (placeholder, sync root, iCloud).
    CloudBacked(bool),
    /// Operating-system protection flags of the subject.
    Protected(Protection),
}

/// How a fact was established.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "basis", rename_all = "snake_case")]
pub enum Basis {
    /// Directly observed.
    Observed,
    /// Concluded from not finding something. Valid only as far as every searched
    /// source had complete coverage (`CoverageReport::absence_is_provable`).
    Absence {
        /// Sources that were searched.
        searched: BTreeSet<SourceName>,
    },
}

/// Where a fact came from.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Provenance {
    /// Adapter that produced the fact, e.g. `macos.getattrlistbulk`.
    pub source: SourceName,
    /// When it was observed.
    pub observed_at: Timestamp,
}

/// One immutable fact about one subject.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Evidence {
    /// What the fact is about.
    pub subject: Subject,
    /// The fact.
    pub fact: Fact,
    /// How it was established.
    pub basis: Basis,
    /// Where and when it was observed.
    pub provenance: Provenance,
}

impl Evidence {
    /// Canonical bytes: compact JSON with fixed field order. Every map in the
    /// model is a `BTreeMap`/`BTreeSet` and no floats are used, so the output is
    /// deterministic. Golden tests pin the exact bytes and hashes.
    ///
    /// # Panics
    ///
    /// Never in practice: serializing these types cannot fail (no maps with
    /// non-string keys, no failing `Serialize` impls).
    pub fn canonical_bytes(&self) -> Vec<u8> {
        serde_json::to_vec(self)
            .unwrap_or_else(|e| unreachable!("evidence serialization is infallible: {e}"))
    }

    /// The content address of this record.
    pub fn id(&self) -> EvidenceId {
        let mut hasher = blake3::Hasher::new();
        hasher.update(HASH_DOMAIN);
        hasher.update(&self.canonical_bytes());
        EvidenceId(*hasher.finalize().as_bytes())
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;
    use crate::{AppIdScheme, FileId};

    fn source(name: &str) -> SourceName {
        SourceName::new(name).unwrap_or_else(|_| unreachable!())
    }

    fn sample() -> Evidence {
        Evidence {
            subject: Subject::File {
                identity: FileIdentity {
                    volume: VolumeId::new("A1B2").unwrap_or_else(|_| unreachable!()),
                    file: FileId::new(42),
                },
            },
            fact: Fact::OwningApplication(AppId {
                scheme: AppIdScheme::BundleId,
                value: UntrustedText::new("com.example.App"),
            }),
            basis: Basis::Observed,
            provenance: Provenance {
                source: source("macos.library-convention"),
                observed_at: "2026-10-07T09:00:00Z"
                    .parse()
                    .unwrap_or_else(|_| unreachable!()),
            },
        }
    }

    #[test]
    fn canonical_bytes_are_pinned() {
        // Golden: changing the model's serialization must be a deliberate,
        // reviewed change (and a HASH_DOMAIN bump), never an accident.
        let expected = concat!(
            r#"{"subject":{"type":"file","identity":{"volume":"A1B2","file":"42"}},"#,
            r#""fact":{"fact":"owning_application","value":{"scheme":"bundle_id","value":{"text":"com.example.App","truncated":false}}},"#,
            r#""basis":{"basis":"observed"},"#,
            r#""provenance":{"source":"macos.library-convention","observed_at":"2026-10-07T09:00:00Z"}}"#
        );
        assert_eq!(
            String::from_utf8_lossy(&sample().canonical_bytes()),
            expected
        );
    }

    #[test]
    fn id_is_domain_separated_blake3_of_canonical_bytes() {
        let e = sample();
        let mut manual = blake3::Hasher::new();
        manual.update(b"lumen.evidence/1\0");
        manual.update(&e.canonical_bytes());
        assert_eq!(e.id().as_bytes(), manual.finalize().as_bytes());
        assert_ne!(
            e.id().as_bytes(),
            blake3::hash(&e.canonical_bytes()).as_bytes(),
            "prefix must matter"
        );
    }

    #[test]
    fn any_change_changes_the_id() {
        let base = sample();
        let mut later = base.clone();
        later.provenance.observed_at = "2026-10-07T09:00:01Z"
            .parse()
            .unwrap_or_else(|_| unreachable!());
        let mut inferred = base.clone();
        inferred.basis = Basis::Absence {
            searched: BTreeSet::from([source("macos.applications")]),
        };
        assert_ne!(base.id(), later.id());
        assert_ne!(base.id(), inferred.id());
    }

    #[test]
    fn id_text_form_round_trips_and_is_strict() -> Result<(), EvidenceIdError> {
        let id = sample().id();
        let text = id.to_string();
        assert!(text.starts_with("b3:") && text.len() == 67);
        assert_eq!(text.parse::<EvidenceId>()?, id);
        for bad in [
            "",
            "b3:",
            "sha256:00",
            &text.to_uppercase(),
            &text[..66],
            &format!("{text}0"),
        ] {
            assert!(bad.parse::<EvidenceId>().is_err(), "{bad}");
        }
        Ok(())
    }

    #[test]
    fn evidence_round_trips_and_keeps_its_id() -> serde_json::Result<()> {
        let e = sample();
        let back: Evidence = serde_json::from_slice(&e.canonical_bytes())?;
        assert_eq!(back.id(), e.id());
        Ok(())
    }

    proptest! {
        #[test]
        fn id_is_deterministic_and_injective_over_times(a in 0i64..4_000_000_000, b in 0i64..4_000_000_000) {
            let mut ea = sample();
            ea.provenance.observed_at = Timestamp::from_unix(a, 0).map_err(|e| TestCaseError::fail(e.to_string()))?;
            let mut eb = sample();
            eb.provenance.observed_at = Timestamp::from_unix(b, 0).map_err(|e| TestCaseError::fail(e.to_string()))?;
            prop_assert_eq!(ea.id(), ea.clone().id());
            prop_assert_eq!(ea.id() == eb.id(), a == b);
        }
    }
}
