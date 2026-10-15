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
//! Installed applications come from inventory sources: [`summarize`] records
//! which were read completely and what they found, and
//! [`application_evidence`] states whether the applications that evidence
//! names as owners are installed (roadmap 10.3).
//!
//! Ownership is not in scanned entries; [`ownership_evidence`] states what an
//! `OwnershipProbe` established when the item is assessed (roadmap 10.2b).
//!
//! Facts come only from metadata the scanner actually read. An entry whose
//! metadata was not read (or a directory whose listing was not) yields nothing:
//! unknown is never stated as "unprotected" or "local" (ADR-0008).

use std::collections::{BTreeMap, BTreeSet};

use lumen_domain::{
    AppId, Basis, Evidence, Fact, FileIdentity, FilesystemEntry, Provenance, SourceName,
    SourceStatus, Subject, Timestamp,
};

use crate::ports::InventoryReport;

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

/// The fact that the file `identity` is (or is not) owned by the current user,
/// as `source` established it. An unknown owner states nothing.
pub fn ownership_evidence(
    identity: &FileIdentity,
    owned: Option<bool>,
    source: &SourceName,
    observed_at: Timestamp,
) -> Option<Evidence> {
    Some(Evidence {
        subject: Subject::File {
            identity: identity.clone(),
        },
        fact: Fact::OwnedByCurrentUser(owned?),
        basis: Basis::Observed,
        provenance: Provenance {
            source: source.clone(),
            observed_at,
        },
    })
}

/// The source named in the provenance of inventory conclusions.
pub const INVENTORY_SOURCE: &str = "lumen.inventory";

/// What a set of inventory reports establishes (roadmap 10.3).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Inventory {
    /// Sources read completely; only their silence counts as absence, and they
    /// become `Context.complete_sources`.
    pub complete_sources: BTreeSet<SourceName>,
    /// Every identifier of every application any source found, with a source
    /// that reported it.
    pub found: BTreeMap<AppId, SourceName>,
}

/// Summarizes `reports`.
pub fn summarize(reports: &[InventoryReport]) -> Inventory {
    let mut inventory = Inventory::default();
    for report in reports {
        if report.status == SourceStatus::Complete {
            inventory.complete_sources.insert(report.source.clone());
        }
        for app in &report.applications {
            for id in &app.ids {
                inventory
                    .found
                    .entry(id.clone())
                    .or_insert_with(|| report.source.clone());
            }
        }
    }
    inventory
}

/// Whether each application in `apps` is installed, as `inventory` shows.
///
/// A found application is present (observed by the source that reported it).
/// One no source found is absent only as far as complete sources were
/// searched (`Basis::Absence`); with no complete source nothing is stated,
/// because silence from a partial or unavailable source proves nothing
/// (ADR-0008).
pub fn application_evidence<'a>(
    apps: impl IntoIterator<Item = &'a AppId>,
    inventory: &Inventory,
    observed_at: Timestamp,
) -> Vec<Evidence> {
    let Ok(conclusion) = SourceName::new(INVENTORY_SOURCE) else {
        return Vec::new();
    };
    let wanted: BTreeSet<&AppId> = apps.into_iter().collect();
    wanted
        .into_iter()
        .filter_map(|app| {
            let (present, basis, source) = match inventory.found.get(app) {
                Some(source) => (true, Basis::Observed, source.clone()),
                None if !inventory.complete_sources.is_empty() => (
                    false,
                    Basis::Absence {
                        searched: inventory.complete_sources.clone(),
                    },
                    conclusion.clone(),
                ),
                None => return None,
            };
            Some(Evidence {
                subject: Subject::Application { id: app.clone() },
                fact: Fact::ApplicationPresent {
                    app: app.clone(),
                    present,
                },
                basis,
                provenance: Provenance {
                    source,
                    observed_at,
                },
            })
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
    fn ownership_is_stated_only_when_known() {
        let identity = entry(AccessState::Readable).identity().clone();
        let source = SourceName::new("unix.owner").unwrap_or_else(|_| unreachable!());
        let mine = ownership_evidence(&identity, Some(true), &source, at(1));
        assert_eq!(mine.map(|e| e.fact), Some(Fact::OwnedByCurrentUser(true)));
        assert_eq!(ownership_evidence(&identity, None, &source, at(1)), None);
    }

    fn app_id(value: &str) -> AppId {
        AppId {
            scheme: lumen_domain::AppIdScheme::BundleId,
            value: lumen_domain::UntrustedText::new(value),
        }
    }

    fn report(source: &str, status: SourceStatus, apps: &[&str]) -> InventoryReport {
        InventoryReport {
            source: SourceName::new(source).unwrap_or_else(|_| unreachable!()),
            status,
            applications: apps
                .iter()
                .map(|id| lumen_domain::Application {
                    ids: BTreeSet::from([app_id(id)]),
                    name: None,
                    version: None,
                    vendor: None,
                    signature: None,
                    install_locations: Vec::new(),
                    sources: BTreeSet::new(),
                })
                .collect(),
            services: Vec::new(),
            packages: Vec::new(),
        }
    }

    #[test]
    fn found_apps_are_present_and_missing_ones_absent_only_from_complete_sources() {
        let inventory = summarize(&[
            report(
                "macos.applications",
                SourceStatus::Complete,
                &["com.example.kept"],
            ),
            report(
                "macos.launchd-plists",
                SourceStatus::Partial,
                &["com.example.agent"],
            ),
        ]);
        assert_eq!(
            inventory.complete_sources,
            BTreeSet::from([
                SourceName::new("macos.applications").unwrap_or_else(|_| unreachable!())
            ])
        );
        let (kept, agent, gone) = (
            app_id("com.example.kept"),
            app_id("com.example.agent"),
            app_id("com.example.gone"),
        );
        let evidence = application_evidence([&kept, &agent, &gone], &inventory, at(1));
        let by_app: BTreeMap<&AppId, &Evidence> = evidence
            .iter()
            .filter_map(|e| match &e.fact {
                Fact::ApplicationPresent { app, .. } => Some((app, e)),
                _ => None,
            })
            .collect();
        assert_eq!(by_app.len(), 3);
        assert_eq!(by_app[&kept].basis, Basis::Observed);
        assert_eq!(
            by_app[&agent].provenance.source.as_str(),
            "macos.launchd-plists",
            "found by a partial source still counts as present"
        );
        assert_eq!(
            by_app[&gone].fact,
            Fact::ApplicationPresent {
                app: gone.clone(),
                present: false
            }
        );
        assert_eq!(
            by_app[&gone].basis,
            Basis::Absence {
                searched: inventory.complete_sources.clone()
            }
        );
    }

    #[test]
    fn without_a_complete_source_absence_is_never_stated() {
        let inventory = summarize(&[report("macos.applications", SourceStatus::Unavailable, &[])]);
        let gone = app_id("com.example.gone");
        assert_eq!(application_evidence([&gone], &inventory, at(1)), []);
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
