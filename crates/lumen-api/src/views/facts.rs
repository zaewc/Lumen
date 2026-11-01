//! The facts a candidate's decision saw (ADR-0053): typed, with their basis
//! and source. Facts are evidence, not confidence. Nothing here is scanned,
//! opened or sent to a model.

use lumen_application::assess::Assessed;
use lumen_domain::{AppDataKind, AppId, Basis, Evidence, Fact, Subject, Timestamp, UntrustedText};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// What kind of location the platform documents an app data folder as.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum AppDataKindView {
    /// Data the app can re-create or download.
    Caches,
    /// A sandboxed app's container: its documents and state.
    Container,
    /// Files the app needs to run.
    ApplicationSupport,
    /// The app's preferences.
    Preferences,
    /// Window state restored at launch.
    SavedApplicationState,
    /// Web storage, cookies or scripts of the app's web views.
    WebData,
}

/// One fact, typed. Untrusted text is escaped for display.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FactKindView {
    /// It belongs to this application.
    OwningApplication {
        /// The application's identifier.
        app: String,
    },
    /// Whether an application that owns it is installed.
    ApplicationPresent {
        /// The application's identifier.
        app: String,
        /// Whether it was found.
        present: bool,
    },
    /// It is an application data location of a documented kind.
    AppDataLocation {
        /// The kind.
        kind: AppDataKindView,
    },
    /// It matches a developer knowledge-base entry.
    KnowledgeBaseMatch {
        /// The entry's ID.
        entry: String,
    },
    /// A vendor declared it a disposable cache.
    VendorDeclaredCache {
        /// Who declared it.
        declared_by: String,
    },
    /// The operating system's protection flags on it.
    Protected {
        /// System Integrity Protection or an equivalent.
        system_restricted: bool,
        /// Cannot be unlinked, or immutable.
        undeletable: bool,
        /// Windows' system attribute.
        system_attribute: bool,
        /// Hidden from normal listings.
        hidden: bool,
    },
    /// Whether it is synced with a cloud service, or holds or lies in synced
    /// content.
    CloudBacked {
        /// Whether it is.
        cloud_backed: bool,
    },
    /// It holds a web browser profile's state.
    BrowserProfile {
        /// The entry that showed it, relative to the item.
        marker: String,
    },
    /// A large folder no other evidence names; says nothing about what it is.
    LargeUnrecognised {
        /// What removing it would free, as the scan measured it, in bytes.
        reclaimable_bytes: String,
    },
    /// Whether a running process uses it, every process checked.
    InUse {
        /// Whether one does.
        in_use: bool,
    },
    /// No process Lumen could see uses it, but not every process was
    /// visible: not proof of non-use.
    NotInUseByVisibleProcesses {
        /// How many processes could not be checked, as a decimal string;
        /// `null` when the platform did not say.
        #[schema(required = true)]
        unseen: Option<String>,
    },
    /// Whether the account Lumen runs as owns it.
    OwnedByCurrentUser {
        /// Whether it does.
        owned: bool,
    },
}

/// How a fact was established.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BasisView {
    /// Directly observed.
    Observed,
    /// Concluded from not finding something in these sources, each of which
    /// was searched completely.
    Absence {
        /// The sources searched.
        searched: Vec<String>,
    },
}

/// One fact the decision saw, with where it came from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct FactView {
    /// The fact.
    pub fact: FactKindView,
    /// How it was established.
    pub basis: BasisView,
    /// The adapter that produced it, such as `macos.support-location.bundle-id`.
    pub source: String,
    /// When it was observed.
    #[schema(value_type = String, format = DateTime)]
    pub observed_at: Timestamp,
}

fn app(id: &AppId) -> String {
    id.value.display()
}

fn text(text: &UntrustedText) -> String {
    text.display()
}

/// The view of `fact`, or `None` for a time (shown in [`TimesView`]) and for
/// a kind this view does not know, which is left out rather than guessed.
fn fact_kind(fact: &Fact) -> Option<FactKindView> {
    Some(match fact {
        Fact::OwningApplication(id) => FactKindView::OwningApplication { app: app(id) },
        Fact::ApplicationPresent { app: id, present } => FactKindView::ApplicationPresent {
            app: app(id),
            present: *present,
        },
        Fact::AppDataLocation { kind } => FactKindView::AppDataLocation {
            kind: match kind {
                AppDataKind::Caches => AppDataKindView::Caches,
                AppDataKind::Container => AppDataKindView::Container,
                AppDataKind::ApplicationSupport => AppDataKindView::ApplicationSupport,
                AppDataKind::Preferences => AppDataKindView::Preferences,
                AppDataKind::SavedApplicationState => AppDataKindView::SavedApplicationState,
                AppDataKind::WebData => AppDataKindView::WebData,
                _ => return None,
            },
        },
        Fact::KnowledgeBaseMatch { entry } => FactKindView::KnowledgeBaseMatch {
            entry: entry.as_str().to_owned(),
        },
        Fact::VendorDeclaredCache { declared_by } => FactKindView::VendorDeclaredCache {
            declared_by: text(declared_by),
        },
        Fact::Protected(flags) => FactKindView::Protected {
            system_restricted: flags.system_restricted,
            undeletable: flags.undeletable,
            system_attribute: flags.system_attribute,
            hidden: flags.hidden,
        },
        Fact::CloudBacked(cloud_backed) => FactKindView::CloudBacked {
            cloud_backed: *cloud_backed,
        },
        Fact::BrowserProfile { marker } => FactKindView::BrowserProfile {
            marker: text(marker),
        },
        Fact::LargeUnrecognised { reclaimable } => FactKindView::LargeUnrecognised {
            reclaimable_bytes: reclaimable.get().to_string(),
        },
        Fact::InUse(in_use) => FactKindView::InUse { in_use: *in_use },
        Fact::NotInUseByVisibleProcesses { unseen } => FactKindView::NotInUseByVisibleProcesses {
            unseen: unseen.map(|count| count.to_string()),
        },
        Fact::OwnedByCurrentUser(owned) => FactKindView::OwnedByCurrentUser { owned: *owned },
        _ => return None,
    })
}

fn fact_view(record: &Evidence) -> Option<FactView> {
    Some(FactView {
        fact: fact_kind(&record.fact)?,
        basis: match &record.basis {
            Basis::Observed => BasisView::Observed,
            Basis::Absence { searched } => BasisView::Absence {
                searched: searched.iter().map(|s| s.as_str().to_owned()).collect(),
            },
        },
        source: record.provenance.source.as_str().to_owned(),
        observed_at: record.provenance.observed_at,
    })
}

/// The facts about `assessed`'s candidate in its bundle, and whether each
/// application that owns it is installed, in the bundle's order. Facts about
/// other subjects belong to their own views.
pub(super) fn facts(assessed: &Assessed) -> Vec<FactView> {
    let candidate = Subject::File {
        identity: assessed.candidate.identity().clone(),
    };
    let evidence = &assessed.input.evidence;
    let owners: Vec<Subject> = evidence
        .iter()
        .filter(|record| record.subject == candidate)
        .filter_map(|record| match &record.fact {
            Fact::OwningApplication(id) => Some(Subject::Application { id: id.clone() }),
            _ => None,
        })
        .collect();
    evidence
        .iter()
        .filter(|record| {
            record.subject == candidate
                || matches!(record.fact, Fact::ApplicationPresent { .. })
                    && owners.contains(&record.subject)
        })
        .filter_map(fact_view)
        .collect()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use lumen_domain::{AccessState, EntryKind, SourceName};

    use super::*;
    use crate::views::fixture::{app_id, assessed, at, file, record};

    #[test]
    fn facts_are_the_candidates_and_its_owners_installation() {
        let owner = app_id("com.example.app");
        let gone = Evidence {
            basis: Basis::Absence {
                searched: BTreeSet::from([
                    SourceName::new("macos.applications").unwrap_or_else(|e| unreachable!("{e}"))
                ]),
            },
            ..record(
                Subject::Application { id: owner.clone() },
                Fact::ApplicationPresent {
                    app: owner.clone(),
                    present: false,
                },
                20,
            )
        };
        let other = app_id("com.example.other");
        let marker = UntrustedText::new("Default/\u{202e}Cookies");
        let seen = assessed(
            EntryKind::Directory,
            AccessState::Readable,
            vec![
                record(file(1), Fact::OwningApplication(owner), 10),
                record(file(1), Fact::LastModified(at(100)), 10),
                record(
                    file(1),
                    Fact::BrowserProfile {
                        marker: marker.clone(),
                    },
                    10,
                ),
                record(
                    file(1),
                    Fact::NotInUseByVisibleProcesses { unseen: None },
                    40,
                ),
                gone,
                record(file(2), Fact::InUse(true), 40),
                record(
                    Subject::Application { id: other.clone() },
                    Fact::ApplicationPresent {
                        app: other,
                        present: true,
                    },
                    20,
                ),
            ],
        );
        let shown = facts(&seen);
        let kinds: Vec<&FactKindView> = shown.iter().map(|fact| &fact.fact).collect();
        assert_eq!(
            kinds,
            [
                &FactKindView::OwningApplication {
                    app: "com.example.app".to_owned()
                },
                &FactKindView::BrowserProfile {
                    marker: marker.display()
                },
                &FactKindView::NotInUseByVisibleProcesses { unseen: None },
                &FactKindView::ApplicationPresent {
                    app: "com.example.app".to_owned(),
                    present: false
                },
            ],
            "times live in `times`; other subjects' facts in their own views"
        );
        assert_ne!(marker.display(), marker.raw(), "untrusted text is escaped");
        let json = serde_json::to_value(&shown[3]).unwrap_or_default();
        assert_eq!(
            json,
            serde_json::json!({
                "fact": {"type": "application_present", "app": "com.example.app", "present": false},
                "basis": {"kind": "absence", "searched": ["macos.applications"]},
                "source": "lumen.scan",
                "observed_at": "1970-01-01T00:00:20Z",
            })
        );
    }
}
