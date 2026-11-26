//! Which owned items belong to security software (ADR-0060 rule 4, ADR-0070
//! step 5), as [`Fact::OwningSecuritySoftware`] for the policy's
//! `protect.security-software`.
//!
//! The fact comes only from positive platform evidence about an installed
//! app: the platform's probe of one of its bundles says it is security
//! software (on macOS, the Endpoint Security entitlement in its signature).
//! It is added to an item only when the item already carries that app's
//! [`Fact::OwningApplication`]. A display name, a vendor string or an AI
//! answer never produces it, and a probe that cannot answer produces nothing,
//! neither this fact nor a claim that the owner is not security software.
//!
//! The probe is passed in, so this stays free of IO.

use std::collections::BTreeMap;

use lumen_domain::{
    AppId, Application, Basis, Evidence, Fact, Provenance, RawPath, SourceName, Subject, Timestamp,
};

/// The most owning applications one gathering probes, as for usage.
pub const MAX_PROBED_OWNERS: usize = 200;

/// `OwningSecuritySoftware` evidence for every item in `evidence` whose
/// owning application, installed as one of `apps`, `probe` says is security
/// software: `Some(true)` for at least one of its bundles. `None` or
/// `Some(false)` from every bundle adds nothing. Each owner is probed once,
/// at most [`MAX_PROBED_OWNERS`] owners, in a stable order.
pub fn security_owner_evidence(
    apps: &[Application],
    evidence: &[Evidence],
    source: &SourceName,
    now: Timestamp,
    probe: &mut dyn FnMut(&RawPath) -> Option<bool>,
) -> Vec<Evidence> {
    // Each owner and the items it owns, in a stable order.
    let mut owned: BTreeMap<&AppId, Vec<&Subject>> = BTreeMap::new();
    for record in evidence {
        if let (Fact::OwningApplication(app), Subject::File { .. }) =
            (&record.fact, &record.subject)
        {
            owned.entry(app).or_default().push(&record.subject);
        }
    }
    let mut found = Vec::new();
    for (owner, subjects) in owned.into_iter().take(MAX_PROBED_OWNERS) {
        let security = apps
            .iter()
            .filter(|app| app.ids.contains(owner))
            .flat_map(|app| &app.install_locations)
            .any(|bundle| probe(bundle) == Some(true));
        if !security {
            continue;
        }
        for subject in subjects {
            found.push(Evidence {
                subject: subject.clone(),
                fact: Fact::OwningSecuritySoftware(owner.clone()),
                basis: Basis::Observed,
                provenance: Provenance {
                    source: source.clone(),
                    observed_at: now,
                },
            });
        }
    }
    found.sort_by_key(Evidence::id);
    found.dedup_by_key(|record| record.id());
    found
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use lumen_domain::{AppIdScheme, FileId, FileIdentity, UntrustedText, VolumeId};

    use super::*;

    fn id(value: &str) -> AppId {
        AppId {
            scheme: AppIdScheme::BundleId,
            value: UntrustedText::new(value),
        }
    }

    fn raw(path: &str) -> RawPath {
        RawPath::from_unix_bytes(path.as_bytes().to_vec()).unwrap_or_else(|_| unreachable!())
    }

    fn app(bundle_id: &str, bundle: &str, name: &str) -> Application {
        Application {
            ids: BTreeSet::from([id(bundle_id)]),
            name: Some(UntrustedText::new(name)),
            version: None,
            vendor: None,
            signature: None,
            install_locations: vec![raw(bundle)],
            sources: BTreeSet::new(),
            uninstall: None,
        }
    }

    fn file(n: u128) -> Subject {
        Subject::File {
            identity: FileIdentity {
                volume: VolumeId::new("dev:1").unwrap_or_else(|_| unreachable!()),
                file: FileId::new(n),
            },
        }
    }

    fn source() -> SourceName {
        SourceName::new("test.codesign").unwrap_or_else(|_| unreachable!())
    }

    fn record(subject: Subject, fact: Fact) -> Evidence {
        Evidence {
            subject,
            fact,
            basis: Basis::Observed,
            provenance: Provenance {
                source: source(),
                observed_at: Timestamp::UNIX_EPOCH,
            },
        }
    }

    fn owned(n: u128, owner: &str) -> Evidence {
        record(file(n), Fact::OwningApplication(id(owner)))
    }

    /// The probe's answer for each bundle; every other bundle is unknown.
    fn answers(known: &'static [(&'static str, bool)]) -> impl FnMut(&RawPath) -> Option<bool> {
        move |bundle| {
            known
                .iter()
                .find(|(path, _)| path.as_bytes() == bundle.as_bytes())
                .map(|(_, answer)| *answer)
        }
    }

    fn produce(
        apps: &[Application],
        evidence: &[Evidence],
        probe: &mut dyn FnMut(&RawPath) -> Option<bool>,
    ) -> Vec<(Subject, Fact)> {
        security_owner_evidence(apps, evidence, &source(), Timestamp::UNIX_EPOCH, probe)
            .into_iter()
            .map(|record| (record.subject, record.fact))
            .collect()
    }

    #[test]
    fn every_item_an_entitled_app_owns_carries_the_fact() {
        let apps = [
            app("com.example.guard", "/Applications/Guard.app", "Guard"),
            app("com.example.notes", "/Applications/Notes.app", "Notes"),
        ];
        let evidence = [
            owned(1, "com.example.guard"),
            owned(2, "com.example.guard"),
            owned(3, "com.example.notes"),
        ];
        let mut probe = answers(&[
            ("/Applications/Guard.app", true),
            ("/Applications/Notes.app", false),
        ]);
        let found = produce(&apps, &evidence, &mut probe);
        let fact = Fact::OwningSecuritySoftware(id("com.example.guard"));
        assert_eq!(found.len(), 2);
        assert!(found.contains(&(file(1), fact.clone())));
        assert!(found.contains(&(file(2), fact)));
    }

    #[test]
    fn an_unknown_answer_a_name_or_a_missing_install_adds_nothing() {
        // Named like security software, but the probe cannot tell.
        let unreadable = [app(
            "com.example.antivirus",
            "/Applications/Antivirus Security.app",
            "Antivirus Security",
        )];
        let evidence = [owned(1, "com.example.antivirus")];
        assert_eq!(
            produce(&unreadable, &evidence, &mut answers(&[])),
            Vec::new()
        );
        // Not installed: nothing is probed.
        let mut asked = 0;
        let mut counting = |_: &RawPath| {
            asked += 1;
            Some(true)
        };
        assert_eq!(produce(&[], &evidence, &mut counting), Vec::new());
        assert_eq!(asked, 0);
    }

    #[test]
    fn only_items_with_the_owner_fact_and_only_files_are_marked() {
        let apps = [app("com.example.guard", "/Applications/Guard.app", "Guard")];
        let evidence = [
            // About the application itself, not an item it owns.
            record(
                Subject::Application {
                    id: id("com.example.guard"),
                },
                Fact::OwningApplication(id("com.example.guard")),
            ),
            // An item with some other fact only.
            record(file(2), Fact::InUse(false)),
        ];
        let mut probe = answers(&[("/Applications/Guard.app", true)]);
        assert_eq!(produce(&apps, &evidence, &mut probe), Vec::new());
    }

    #[test]
    fn one_entitled_bundle_of_several_is_enough_and_each_owner_is_probed_once() {
        let mut guard = app("com.example.guard", "/Applications/Guard.app", "Guard");
        guard
            .install_locations
            .push(raw("/Users/ana/Applications/Guard.app"));
        let evidence = [owned(1, "com.example.guard"), owned(2, "com.example.guard")];
        let mut asked = Vec::new();
        let mut probe = |bundle: &RawPath| {
            asked.push(bundle.as_bytes().to_vec());
            (bundle.as_bytes() == b"/Users/ana/Applications/Guard.app").then_some(true)
        };
        let found = produce(&[guard], &evidence, &mut probe);
        assert_eq!(found.len(), 2);
        assert_eq!(asked.len(), 2, "each bundle of the one owner, once");
    }
}
