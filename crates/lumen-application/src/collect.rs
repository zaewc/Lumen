//! Evidence from what the scan observed (roadmap 10.2, ADR-0014).
//!
//! For each stored entry the collector states, about the file subject:
//!
//! - its operating-system protections, always, so the policy's hard-protection
//!   stage sees them (`Fact::Protected`);
//! - whether it is cloud-backed: a provider placeholder or dataless file
//!   (`Fact::CloudBacked`), where removal could propagate to the cloud;
//! - its modification and access times, when the platform reported them.
//!
//! Facts come only from metadata the scanner actually read. An entry whose
//! metadata was not read (or a directory whose listing was not) yields nothing:
//! unknown is never stated as "unprotected" or "local" (ADR-0008).

use lumen_domain::{
    Basis, Evidence, Fact, FilesystemEntry, Provenance, SourceName, Subject, Timestamp,
};

/// The source named in the provenance of scan-derived facts.
pub const SCAN_SOURCE: &str = "lumen.scan";

/// The facts the scan established about `entry`, observed at `observed_at`.
pub fn filesystem_evidence(entry: &FilesystemEntry, observed_at: Timestamp) -> Vec<Evidence> {
    if !entry.access.is_observed() {
        return Vec::new();
    }
    let Ok(source) = SourceName::new(SCAN_SOURCE) else {
        return Vec::new();
    };
    let subject = Subject::File {
        identity: entry.identity().clone(),
    };
    let cloud = entry.size.flags.cloud_placeholder || entry.size.flags.dataless;
    let mut facts = vec![Fact::Protected(entry.protection), Fact::CloudBacked(cloud)];
    facts.extend(entry.times.modified.map(Fact::LastModified));
    facts.extend(entry.times.accessed.map(Fact::LastAccessed));
    facts
        .into_iter()
        .map(|fact| Evidence {
            subject: subject.clone(),
            fact,
            basis: Basis::Observed,
            provenance: Provenance {
                source: source.clone(),
                observed_at,
            },
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;

    use lumen_domain::{
        AccessState, ByteCount, DenialReason, EntryKind, EntryTimes, FileId, FileIdentity,
        Protection, RawPath, SizeFacts, SizeFlags, VolumeId,
    };

    use super::*;

    fn at(secs: i64) -> Timestamp {
        Timestamp::from_unix(secs, 0).unwrap_or_else(|_| unreachable!())
    }

    fn entry(access: AccessState) -> FilesystemEntry {
        FilesystemEntry {
            path: RawPath::from_unix_bytes(b"/Users/ana/Library/Caches/x".to_vec())
                .unwrap_or_else(|_| unreachable!()),
            kind: EntryKind::File,
            size: SizeFacts {
                identity: FileIdentity {
                    volume: VolumeId::new("v").unwrap_or_else(|_| unreachable!()),
                    file: FileId::new(7),
                },
                logical: ByteCount::new(10),
                allocated: ByteCount::new(4096),
                private: None,
                clone_id: None,
                link_count: NonZeroU32::MIN,
                flags: SizeFlags::default(),
            },
            times: EntryTimes {
                modified: Some(at(1_700_000_000)),
                accessed: None,
                changed: None,
                created: None,
            },
            protection: Protection::default(),
            access,
        }
    }

    fn facts(evidence: &[Evidence]) -> Vec<Fact> {
        evidence.iter().map(|e| e.fact.clone()).collect()
    }

    #[test]
    fn observed_metadata_becomes_facts_about_the_file() {
        let mut protected = entry(AccessState::Readable);
        protected.protection.system_restricted = true;
        protected.size.flags.dataless = true;
        let evidence = filesystem_evidence(&protected, at(1_800_000_000));
        assert_eq!(
            facts(&evidence),
            [
                Fact::Protected(protected.protection),
                Fact::CloudBacked(true),
                Fact::LastModified(at(1_700_000_000)),
            ]
        );
        for e in &evidence {
            assert_eq!(
                e.subject,
                Subject::File {
                    identity: protected.identity().clone()
                }
            );
            assert_eq!(e.basis, Basis::Observed);
            assert_eq!(e.provenance.source.as_str(), SCAN_SOURCE);
        }
    }

    #[test]
    fn an_unprotected_local_file_says_so_explicitly() {
        let evidence = filesystem_evidence(&entry(AccessState::Readable), at(1));
        assert!(facts(&evidence).contains(&Fact::Protected(Protection::default())));
        assert!(facts(&evidence).contains(&Fact::CloudBacked(false)));
    }

    #[test]
    fn unread_metadata_states_nothing() {
        for access in [
            AccessState::NotScanned,
            AccessState::Denied(DenialReason::Posix),
        ] {
            assert_eq!(filesystem_evidence(&entry(access), at(1)), []);
        }
    }
}
