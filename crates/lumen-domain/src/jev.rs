//! The request Jev receives (`jev.request/1`, docs/ai/model-contract.md).
//!
//! A request describes exactly one candidate with metadata only: never file
//! contents. Every attacker-controllable string (file names, display names,
//! vendor declarations) appears only as an [`UntrustedText`] in an evidence
//! `value` or in `untrusted_strings`, so a prompt can label it as data.
//! Integers are decimal strings, as everywhere in Lumen.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{ByteCount, Category, EvidenceId, ItemRef, PayloadHash, SchemaVersion, UntrustedText};

/// Domain separator for request hashes.
const REQUEST_HASH_DOMAIN: &[u8] = b"lumen.jev.request/1\0";

/// Schema name of requests.
pub const JEV_REQUEST_SCHEMA: &str = "jev.request";

/// What one evidence item says, as a closed vocabulary.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum EvidenceKind {
    /// The application that owns the candidate.
    OwningApplication,
    /// Whether that application is installed (`true`/`false`).
    ApplicationInstalled,
    /// Whole days since the content was modified.
    LastModifiedDays,
    /// Whole days since the candidate was accessed (weak).
    LastAccessDays,
    /// Whether a visible process uses the candidate (`true`/`false`).
    ActiveProcessReference,
    /// A vendor declared the candidate's contents disposable.
    VendorDeclaredCache,
    /// The knowledge-base entry the candidate matched.
    KnowledgeBaseEntry,
    /// Whether the candidate regenerates on demand (`true`/`false`).
    Regeneratable,
    /// The kind of application data location (e.g. `caches`).
    AppDataLocation,
    /// Code-signature status of the candidate.
    Signature,
}

/// One evidence item in a request.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RequestEvidence {
    /// The evidence record this item summarises; a judgment may cite it.
    pub id: EvidenceId,
    /// What the item says.
    pub kind: EvidenceKind,
    /// The value, as text; untrusted when it comes from the filesystem or an app.
    pub value: UntrustedText,
}

/// The candidate itself.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CandidateSummary {
    /// Artifact category.
    pub category: Category,
    /// Redacted display path (`~` for the home directory, user names removed).
    pub path_display: UntrustedText,
    /// Allocated size.
    pub size_bytes: ByteCount,
    /// Bytes removing it would free now.
    pub reclaimable_bytes: ByteCount,
}

/// Why observation of a candidate was incomplete.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum CoverageNote {
    /// Not every running process could be checked.
    ProcessesPartlyVisible,
    /// An inventory source was read only partly.
    InventoryIncomplete,
    /// Part of the candidate could not be read.
    ContentsPartlyUnreadable,
}

/// How completely the candidate was observed.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CoverageSummary {
    /// Whether every relevant source was read completely.
    pub complete: bool,
    /// What was incomplete.
    pub notes: BTreeSet<CoverageNote>,
}

/// Attacker-controllable strings, each in a labelled field.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum UntrustedField {
    /// The candidate's file or directory name.
    FileName,
    /// The owning application's display name.
    BundleDisplayName,
    /// A vendor's declaration text (e.g. a Disk Cleanup handler name).
    VendorDeclaration,
}

/// Everything Jev may see about one candidate (`jev.request/1`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct JevRequest {
    /// `jev.request/1.x`.
    pub schema_version: SchemaVersion,
    /// Opaque reference the judgment must echo.
    pub item_ref: ItemRef,
    /// The candidate.
    pub candidate: CandidateSummary,
    /// Evidence about it; a judgment may cite only these IDs.
    pub evidence: Vec<RequestEvidence>,
    /// How completely it was observed.
    pub coverage: CoverageSummary,
    /// Attacker-controllable strings, labelled.
    pub untrusted_strings: BTreeMap<UntrustedField, UntrustedText>,
}

impl JevRequest {
    /// The schema version this code writes.
    ///
    /// # Errors
    ///
    /// Never in practice: the name and version are constants that are valid.
    pub fn current_schema() -> Result<SchemaVersion, crate::VersionError> {
        SchemaVersion::new(JEV_REQUEST_SCHEMA, 1, 0)
    }

    /// Domain-separated BLAKE3 of the request's canonical JSON, recorded in
    /// traces and used to key recorded answers (cassettes).
    pub fn hash(&self) -> PayloadHash {
        let mut hasher = blake3::Hasher::new();
        hasher.update(REQUEST_HASH_DOMAIN);
        hasher.update(
            &serde_json::to_vec(self)
                .unwrap_or_else(|e| unreachable!("request serialization is infallible: {e}")),
        );
        PayloadHash::from_bytes(*hasher.finalize().as_bytes())
    }

    /// The evidence IDs a judgment may cite.
    pub fn evidence_ids(&self) -> BTreeSet<EvidenceId> {
        self.evidence.iter().map(|e| e.id).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(byte: u8) -> EvidenceId {
        EvidenceId::from_bytes([byte; 32])
    }

    fn sample() -> Result<JevRequest, Box<dyn std::error::Error>> {
        Ok(JevRequest {
            schema_version: JevRequest::current_schema()?,
            item_ref: ItemRef::new("c_1")?,
            candidate: CandidateSummary {
                category: Category::AppCache,
                path_display: UntrustedText::new("~/Library/Caches/com.example.App"),
                size_bytes: ByteCount::new(1_288_490_188),
                reclaimable_bytes: ByteCount::new(1_288_490_188),
            },
            evidence: vec![
                RequestEvidence {
                    id: id(1),
                    kind: EvidenceKind::OwningApplication,
                    value: UntrustedText::new("com.example.App"),
                },
                RequestEvidence {
                    id: id(2),
                    kind: EvidenceKind::ApplicationInstalled,
                    value: UntrustedText::new("false"),
                },
            ],
            coverage: CoverageSummary {
                complete: false,
                notes: BTreeSet::from([CoverageNote::ProcessesPartlyVisible]),
            },
            untrusted_strings: BTreeMap::from([(
                UntrustedField::FileName,
                UntrustedText::new("com.example.App"),
            )]),
        })
    }

    #[test]
    fn requests_round_trip_with_decimal_sizes() -> Result<(), Box<dyn std::error::Error>> {
        let request = sample()?;
        let json = serde_json::to_value(&request)?;
        assert_eq!(json["schema_version"], "jev.request/1.0");
        assert_eq!(json["candidate"]["size_bytes"], "1288490188");
        assert_eq!(json["evidence"][1]["kind"], "application_installed");
        assert_eq!(json["coverage"]["notes"][0], "processes_partly_visible");
        assert!(json["untrusted_strings"].get("file_name").is_some());
        let back: JevRequest = serde_json::from_value(json)?;
        assert_eq!(back, request);
        assert_eq!(back.evidence_ids(), BTreeSet::from([id(1), id(2)]));
        Ok(())
    }

    #[test]
    fn requests_conform_to_their_schema() -> Result<(), Box<dyn std::error::Error>> {
        let schema = serde_json::to_value(schemars::schema_for!(JevRequest))?;
        let validator = jsonschema::validator_for(&schema)?;
        let errors: Vec<String> = validator
            .iter_errors(&serde_json::to_value(sample()?)?)
            .map(|e| e.to_string())
            .collect();
        assert_eq!(errors, Vec::<String>::new());
        Ok(())
    }

    #[test]
    fn hashes_are_stable_and_sensitive() -> Result<(), Box<dyn std::error::Error>> {
        let request = sample()?;
        assert_eq!(request.hash(), sample()?.hash());
        let mut other = sample()?;
        other.item_ref = ItemRef::new("c_2")?;
        assert_ne!(request.hash(), other.hash());
        let mut plain = blake3::Hasher::new();
        plain.update(&serde_json::to_vec(&request)?);
        assert_ne!(
            request.hash().as_bytes(),
            plain.finalize().as_bytes(),
            "domain-separated"
        );
        Ok(())
    }

    #[test]
    fn unknown_fields_are_rejected() -> Result<(), Box<dyn std::error::Error>> {
        let mut json = serde_json::to_value(sample()?)?;
        json["command"] = serde_json::json!("rm -rf ~");
        assert!(serde_json::from_value::<JevRequest>(json).is_err());
        Ok(())
    }
}
