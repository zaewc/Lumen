//! The quarantine manifest (`quarantine.manifest/1`, ADR-0015).
//!
//! One manifest per quarantined item records everything needed to explain the
//! move and to put the item back exactly: the original path as raw bytes, the
//! observed entry, the decision that allowed it, and the user's confirmation.
//! It is written next to the item in its store, so a lost `ledger.db` can be
//! rebuilt from the stores. The item's lifecycle state lives in the journal,
//! not here: a manifest is written once and never changed.

use serde::{Deserialize, Serialize};

use crate::{
    DeviceId, FilesystemEntry, OperationId, PayloadHash, PlanId, PolicyDecision, RawPath,
    SchemaVersion, Timestamp, VersionError,
};

/// Schema name of manifests.
pub const MANIFEST_SCHEMA: &str = "quarantine.manifest";

/// Unix ownership and permissions at move time, for an exact restore.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct UnixOwnership {
    /// `st_mode` permission and type bits.
    pub mode: u32,
    /// Owner user ID.
    pub uid: u32,
    /// Owner group ID.
    pub gid: u32,
}

/// Everything recorded about one quarantined item.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct QuarantineManifest {
    /// `quarantine.manifest/1.x`.
    pub schema_version: SchemaVersion,
    /// The operation; also the item's name in its store.
    pub operation: OperationId,
    /// The confirmed plan the item belonged to.
    pub plan: PlanId,
    /// The device the item was quarantined on.
    pub device: DeviceId,
    /// When the item was moved.
    pub moved_at: Timestamp,
    /// The original path, exactly as the platform reported it.
    pub original_path: RawPath,
    /// The item as observed right before the move (identity, kind, size facts,
    /// times, protection).
    pub entry: FilesystemEntry,
    /// Unix ownership and permissions, where the platform has them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unix: Option<UnixOwnership>,
    /// The decision that proposed the move, with its policy version and
    /// evidence hash.
    pub decision: PolicyDecision,
    /// When the user confirmed the plan.
    pub confirmed_at: Timestamp,
    /// The Jev consultation the decision used, by request hash, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jev_request: Option<PayloadHash>,
}

impl QuarantineManifest {
    /// The schema version this code writes.
    ///
    /// # Errors
    ///
    /// Never in practice: the name and version are constants that are valid.
    pub fn current_schema() -> Result<SchemaVersion, VersionError> {
        SchemaVersion::new(MANIFEST_SCHEMA, 1, 0)
    }

    /// Whether this code can read the manifest: same schema name and major
    /// version.
    pub fn is_readable(&self) -> bool {
        Self::current_schema().is_ok_and(|current| self.schema_version.is_readable_by(&current))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::num::NonZeroU32;

    use super::*;
    use crate::{
        AccessState, ByteCount, EntryKind, EntryTimes, EvidenceHash, FileId, FileIdentity,
        FiredRule, JevEffect, PolicyStage, PolicyVersion, Protection, Risk, RiskLevel, RuleId,
        SizeFacts, SizeFlags, Verdict, VolumeId,
    };

    fn sample() -> Result<QuarantineManifest, Box<dyn std::error::Error>> {
        let path = RawPath::from_unix_bytes(b"/Users/ana/Library/Caches/com.example.App".to_vec())?;
        Ok(QuarantineManifest {
            schema_version: QuarantineManifest::current_schema()?,
            operation: "0190f0d2-6c1e-7a3b-8c4d-5e6f708192a3".parse()?,
            plan: "0190f0d2-6c1e-7a3b-8c4d-5e6f708192a4".parse()?,
            device: "0190f0d2-6c1e-7a3b-8c4d-5e6f708192a5".parse()?,
            moved_at: "2026-10-15T01:00:00Z".parse()?,
            original_path: path.clone(),
            entry: FilesystemEntry {
                path,
                kind: EntryKind::Directory,
                size: SizeFacts {
                    identity: FileIdentity {
                        volume: VolumeId::new("A1B2")?,
                        file: FileId::new(42),
                    },
                    logical: ByteCount::new(4096),
                    allocated: ByteCount::new(4096),
                    private: None,
                    clone_id: None,
                    link_count: NonZeroU32::MIN,
                    flags: SizeFlags::default(),
                },
                times: EntryTimes::default(),
                protection: Protection::default(),
                access: AccessState::Readable,
            },
            unix: Some(UnixOwnership {
                mode: 0o040_700,
                uid: 501,
                gid: 20,
            }),
            decision: PolicyDecision::new(
                Verdict::Quarantine,
                PolicyVersion::new(0, 10, 0),
                EvidenceHash::from_bytes([1; 32]),
                [FiredRule {
                    rule: RuleId::new("classify.app-cache")?,
                    stage: PolicyStage::Classification,
                    evidence: BTreeSet::new(),
                }],
                Risk {
                    level: RiskLevel::Low,
                    factors: BTreeSet::new(),
                },
                JevEffect::None,
            )?,
            confirmed_at: "2026-10-15T00:59:00Z".parse()?,
            jev_request: None,
        })
    }

    #[test]
    fn manifests_round_trip_and_reject_unknown_fields() -> Result<(), Box<dyn std::error::Error>> {
        let manifest = sample()?;
        let json = serde_json::to_value(&manifest)?;
        assert_eq!(json["schema_version"], "quarantine.manifest/1.0");
        let back: QuarantineManifest = serde_json::from_value(json.clone())?;
        assert_eq!(back, manifest);
        assert!(back.is_readable());
        let mut extra = json;
        extra["note"] = serde_json::json!("x");
        assert!(serde_json::from_value::<QuarantineManifest>(extra).is_err());
        Ok(())
    }

    #[test]
    fn manifests_conform_to_their_schema() -> Result<(), Box<dyn std::error::Error>> {
        let schema = serde_json::to_value(schemars::schema_for!(QuarantineManifest))?;
        let validator = jsonschema::validator_for(&schema)?;
        let errors: Vec<String> = validator
            .iter_errors(&serde_json::to_value(sample()?)?)
            .map(|e| e.to_string())
            .collect();
        assert_eq!(errors, Vec::<String>::new());
        Ok(())
    }

    #[test]
    fn other_major_versions_are_not_readable() -> Result<(), Box<dyn std::error::Error>> {
        let mut manifest = sample()?;
        manifest.schema_version = "quarantine.manifest/2.0".parse()?;
        assert!(!manifest.is_readable());
        Ok(())
    }
}
