//! Building `jev.request/1` from what Lumen knows about a candidate (roadmap 8.3,
//! docs/ai/model-contract.md).
//!
//! Only metadata leaves this module, never file contents. The builder:
//!
//! - redacts the display path: the user's home becomes `~`, and the name of any
//!   other user's home directory becomes `<user>`;
//! - turns facts about the subject into closed-vocabulary evidence items;
//!   facts that only protections use (OS flags, cloud backing, ownership) are
//!   not sent, because protected items never reach Jev;
//! - puts attacker-controllable text only in [`UntrustedText`] values and
//!   labelled `untrusted_strings`;
//! - reports incomplete observation as coverage notes, never as absence.

use std::collections::{BTreeMap, BTreeSet};

use lumen_domain::{
    CandidateSummary, Category, CoverageNote, CoverageSummary, Evidence, EvidenceKind, Fact,
    ItemRef, JevRequest, PathFlavor, RawPath, ReclaimEstimate, RequestEvidence, Subject, Timestamp,
    UntrustedField, UntrustedText, VersionError,
};

/// What the builder needs about one candidate.
#[derive(Debug, Clone)]
pub struct Candidate<'a> {
    /// Opaque reference the judgment must echo.
    pub item_ref: ItemRef,
    /// The subject being judged.
    pub subject: &'a Subject,
    /// Its category.
    pub category: Category,
    /// Where it was observed; the first path is shown.
    pub path: Option<&'a RawPath>,
    /// Space it occupies and would free.
    pub size: &'a ReclaimEstimate,
    /// Whether it and everything below it were read completely.
    pub observed_completely: bool,
    /// Whether every inventory searched for its owner was read completely.
    pub inventories_complete: bool,
    /// The evidence bundle's records.
    pub evidence: &'a [Evidence],
}

/// The machine the request is built on.
#[derive(Debug, Clone, Copy)]
pub struct Environment<'a> {
    /// The current user's home directory.
    pub home: Option<&'a RawPath>,
    /// When the request is built, for ages in days.
    pub now: Timestamp,
}

/// Path components and the separator to rejoin them with.
fn split(path: &RawPath) -> (Vec<String>, char) {
    let (separators, join): (&[u8], char) = match path.flavor() {
        PathFlavor::Unix => (b"/", '/'),
        PathFlavor::Windows => (b"\\/", '\\'),
    };
    let parts = path
        .display()
        .as_bytes()
        .split(|b| separators.contains(b))
        .filter(|part| !part.is_empty())
        .map(|part| String::from_utf8_lossy(part).into_owned())
        .collect();
    (parts, join)
}

/// The display path with the home directory as `~` and other users' home
/// directory names replaced.
pub fn redact(path: &RawPath, home: Option<&RawPath>) -> String {
    let (parts, join) = split(path);
    let fold = |s: &str| s.to_lowercase();
    if let Some(home) = home {
        let (home_parts, _) = split(home);
        let below = parts.len() >= home_parts.len()
            && parts
                .iter()
                .zip(&home_parts)
                .all(|(a, b)| fold(a) == fold(b));
        if below {
            let rest: Vec<&str> = parts[home_parts.len()..]
                .iter()
                .map(String::as_str)
                .collect();
            return if rest.is_empty() {
                "~".to_owned()
            } else {
                format!("~{join}{}", rest.join(&join.to_string()))
            };
        }
    }
    let mut out: Vec<String> = parts.clone();
    for (i, part) in parts.iter().enumerate() {
        let parent = fold(part);
        if (parent == "users" || parent == "home") && i + 1 < parts.len() {
            "<user>".clone_into(&mut out[i + 1]);
        }
    }
    let joined = out.join(&join.to_string());
    match path.flavor() {
        PathFlavor::Unix => format!("/{joined}"),
        PathFlavor::Windows => joined,
    }
}

fn item(evidence: &Evidence, kind: EvidenceKind, value: UntrustedText) -> RequestEvidence {
    RequestEvidence {
        id: evidence.id(),
        kind,
        value,
    }
}

fn text(value: impl Into<String>) -> UntrustedText {
    UntrustedText::new(value.into())
}

/// The evidence item for one fact, if it is sent at all.
fn evidence_item(evidence: &Evidence, now: Timestamp) -> Option<RequestEvidence> {
    let days = |at: Timestamp| text(now.whole_days_since(at).max(0).to_string());
    let made = match &evidence.fact {
        Fact::OwningApplication(app) => {
            item(evidence, EvidenceKind::OwningApplication, app.value.clone())
        }
        Fact::ApplicationPresent { present, .. } => {
            // Absence counts only as recorded; the coverage summary says how
            // complete the search was.
            item(
                evidence,
                EvidenceKind::ApplicationInstalled,
                text(present.to_string()),
            )
        }
        Fact::LastModified(at) => item(evidence, EvidenceKind::LastModifiedDays, days(*at)),
        Fact::LastAccessed(at) => item(evidence, EvidenceKind::LastAccessDays, days(*at)),
        Fact::InUse(used) => item(
            evidence,
            EvidenceKind::ActiveProcessReference,
            text(used.to_string()),
        ),
        Fact::NotInUseByVisibleProcesses { .. } => item(
            evidence,
            EvidenceKind::ActiveProcessReference,
            text("false"),
        ),
        Fact::VendorDeclaredCache { declared_by } => item(
            evidence,
            EvidenceKind::VendorDeclaredCache,
            declared_by.clone(),
        ),
        Fact::KnowledgeBaseMatch { entry } => item(
            evidence,
            EvidenceKind::KnowledgeBaseEntry,
            text(entry.as_str()),
        ),
        Fact::AppDataLocation { kind } => item(
            evidence,
            EvidenceKind::AppDataLocation,
            text(serde_json::to_value(kind).ok()?.as_str()?),
        ),
        Fact::Signature(signature) => item(
            evidence,
            EvidenceKind::Signature,
            text(serde_json::to_value(signature.check).ok()?.as_str()?),
        ),
        // Protection inputs never reach Jev, and unknown facts are not sent.
        _ => return None,
    };
    Some(made)
}

/// Builds the request for one candidate.
///
/// # Errors
///
/// Only if the request schema version cannot be constructed (a programming
/// error the tests catch).
pub fn build(candidate: &Candidate<'_>, env: Environment<'_>) -> Result<JevRequest, VersionError> {
    let mut notes = BTreeSet::new();
    if !candidate.observed_completely {
        notes.insert(CoverageNote::ContentsPartlyUnreadable);
    }
    if !candidate.inventories_complete {
        notes.insert(CoverageNote::InventoryIncomplete);
    }
    // Presence facts are about applications; only the subject's owners matter.
    let owners: BTreeSet<_> = candidate
        .evidence
        .iter()
        .filter(|e| &e.subject == candidate.subject)
        .filter_map(|e| match &e.fact {
            Fact::OwningApplication(app) => Some(app),
            _ => None,
        })
        .collect();
    let mut evidence = Vec::new();
    let mut seen = BTreeSet::new();
    for record in candidate.evidence {
        let relevant = &record.subject == candidate.subject
            || matches!(&record.fact, Fact::ApplicationPresent { app, .. } if owners.contains(app));
        if !relevant {
            continue;
        }
        if matches!(record.fact, Fact::NotInUseByVisibleProcesses { .. }) {
            notes.insert(CoverageNote::ProcessesPartlyVisible);
        }
        if let Some(found) = evidence_item(record, env.now)
            && seen.insert(found.id)
        {
            evidence.push(found);
        }
    }
    let path_display = candidate
        .path
        .map_or_else(|| "(unknown)".to_owned(), |p| redact(p, env.home));
    let mut untrusted_strings = BTreeMap::new();
    if let Some(name) = candidate.path.and_then(|p| split(p).0.pop()) {
        untrusted_strings.insert(UntrustedField::FileName, text(name));
    }
    Ok(JevRequest {
        schema_version: JevRequest::current_schema()?,
        item_ref: candidate.item_ref.clone(),
        candidate: CandidateSummary {
            category: candidate.category,
            path_display: text(path_display),
            size_bytes: candidate.size.allocated,
            reclaimable_bytes: candidate.size.reclaimable_now,
        },
        evidence,
        coverage: CoverageSummary {
            complete: notes.is_empty(),
            notes,
        },
        untrusted_strings,
    })
}

#[cfg(test)]
mod tests {
    use lumen_domain::{
        AppId, AppIdScheme, Basis, ByteCount, FileId, FileIdentity, Protection, Provenance,
        SourceName, VolumeId,
    };

    use super::*;

    fn unix(path: &str) -> RawPath {
        RawPath::from_unix_bytes(path.as_bytes().to_vec()).unwrap_or_else(|_| unreachable!())
    }

    fn windows(path: &str) -> RawPath {
        let wide: Vec<u16> = path.encode_utf16().collect();
        RawPath::from_windows_wide(&wide).unwrap_or_else(|_| unreachable!())
    }

    #[test]
    fn paths_are_redacted() {
        let home = unix("/Users/ana");
        assert_eq!(
            redact(&unix("/Users/ana/Library/Caches/x"), Some(&home)),
            "~/Library/Caches/x"
        );
        assert_eq!(redact(&unix("/Users/ana"), Some(&home)), "~");
        assert_eq!(
            redact(&unix("/Users/bob/secret"), Some(&home)),
            "/Users/<user>/secret"
        );
        assert_eq!(
            redact(&unix("/home/carol/.cache"), None),
            "/home/<user>/.cache"
        );
        assert_eq!(
            redact(&unix("/opt/tool/cache"), Some(&home)),
            "/opt/tool/cache"
        );
        let win_home = windows(r"C:\Users\ana");
        assert_eq!(
            redact(&windows(r"c:\users\ANA\AppData\Local\x"), Some(&win_home)),
            r"~\AppData\Local\x"
        );
        assert_eq!(
            redact(&windows(r"C:\Users\bob\x"), Some(&win_home)),
            r"C:\Users\<user>\x"
        );
    }

    fn file(n: u128) -> Subject {
        Subject::File {
            identity: FileIdentity {
                volume: VolumeId::new("v").unwrap_or_else(|_| unreachable!()),
                file: FileId::new(n),
            },
        }
    }

    fn app() -> AppId {
        AppId {
            scheme: AppIdScheme::BundleId,
            value: UntrustedText::new("com.example.App"),
        }
    }

    fn record(subject: Subject, fact: Fact) -> Evidence {
        Evidence {
            subject,
            fact,
            basis: Basis::Observed,
            provenance: Provenance {
                source: SourceName::new("test").unwrap_or_else(|_| unreachable!()),
                observed_at: Timestamp::UNIX_EPOCH,
            },
        }
    }

    fn build_with(
        evidence: &[Evidence],
        observed_completely: bool,
        inventories_complete: bool,
    ) -> Result<JevRequest, VersionError> {
        let path = unix("/Users/ana/Library/Caches/com.example.App");
        let home = unix("/Users/ana");
        let subject = file(1);
        let size = ReclaimEstimate {
            allocated: ByteCount::new(4096),
            reclaimable_now: ByteCount::new(4096),
            ..ReclaimEstimate::default()
        };
        build(
            &Candidate {
                item_ref: ItemRef::new("c_1").unwrap_or_else(|_| unreachable!()),
                subject: &subject,
                category: Category::AppCache,
                path: Some(&path),
                size: &size,
                observed_completely,
                inventories_complete,
                evidence,
            },
            Environment {
                home: Some(&home),
                now: "1970-01-11T00:00:00Z"
                    .parse()
                    .unwrap_or_else(|_| unreachable!()),
            },
        )
    }

    #[test]
    fn facts_become_closed_vocabulary_items() -> Result<(), VersionError> {
        let other_app = AppId {
            scheme: AppIdScheme::BundleId,
            value: UntrustedText::new("com.other"),
        };
        let evidence = [
            record(file(1), Fact::OwningApplication(app())),
            record(
                Subject::Application { id: app() },
                Fact::ApplicationPresent {
                    app: app(),
                    present: false,
                },
            ),
            record(
                Subject::Application {
                    id: other_app.clone(),
                },
                Fact::ApplicationPresent {
                    app: other_app,
                    present: true,
                },
            ),
            record(file(1), Fact::LastAccessed(Timestamp::UNIX_EPOCH)),
            record(file(1), Fact::NotInUseByVisibleProcesses { unseen: None }),
            record(file(1), Fact::Protected(Protection::default())),
            record(file(1), Fact::CloudBacked(false)),
            record(file(1), Fact::OwnedByCurrentUser(true)),
            record(file(2), Fact::InUse(true)),
        ];
        let request = build_with(&evidence, true, true)?;
        let items: Vec<(EvidenceKind, &str)> = request
            .evidence
            .iter()
            .map(|e| (e.kind, e.value.raw()))
            .collect();
        assert_eq!(
            items,
            [
                (EvidenceKind::OwningApplication, "com.example.App"),
                (EvidenceKind::ApplicationInstalled, "false"),
                (EvidenceKind::LastAccessDays, "10"),
                (EvidenceKind::ActiveProcessReference, "false"),
            ],
            "protection facts, other apps and other files are not sent"
        );
        assert_eq!(
            request.coverage.notes,
            BTreeSet::from([CoverageNote::ProcessesPartlyVisible])
        );
        assert!(!request.coverage.complete);
        assert_eq!(
            request.candidate.path_display.raw(),
            "~/Library/Caches/com.example.App"
        );
        assert_eq!(
            request
                .untrusted_strings
                .get(&UntrustedField::FileName)
                .map(UntrustedText::raw),
            Some("com.example.App")
        );
        Ok(())
    }

    #[test]
    fn incomplete_observation_is_reported() -> Result<(), VersionError> {
        let complete = build_with(&[], true, true)?;
        assert!(complete.coverage.complete);
        let partial = build_with(&[], false, false)?;
        assert_eq!(
            partial.coverage.notes,
            BTreeSet::from([
                CoverageNote::InventoryIncomplete,
                CoverageNote::ContentsPartlyUnreadable
            ])
        );
        Ok(())
    }

    #[test]
    fn built_requests_conform_to_the_schema() -> Result<(), Box<dyn std::error::Error>> {
        let request = build_with(
            &[record(file(1), Fact::OwningApplication(app()))],
            true,
            true,
        )?;
        let schema = serde_json::to_value(schemars::schema_for!(JevRequest))?;
        let validator = jsonschema::validator_for(&schema)?;
        let errors: Vec<String> = validator
            .iter_errors(&serde_json::to_value(&request)?)
            .map(|e| e.to_string())
            .collect();
        assert_eq!(errors, Vec::<String>::new());
        Ok(())
    }
}
