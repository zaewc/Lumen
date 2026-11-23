//! The helper's own decision on moving an app owned by another account
//! (ADR-0063 rules 2 and 5).
//!
//! The helper never trusts what a request says about an item. It gathers
//! fresh evidence itself, with full rights ([`HelperEvidence`]), and admits
//! the item only if:
//!
//! - it is still the item the person confirmed: the same path and identity;
//! - it passes every check of ADR-0062's gate ([`gate`]) except the owner
//!   check, with `/Applications` as the only folder, so never a home folder
//!   (`~/Applications`) and never anything under `/System` or `/Library/Apple`;
//! - another account owns it: an app the caller owns is moved by Lumen itself,
//!   without the helper;
//! - no flag protects it: SIP's `restricted`, or `immutable`, `append-only` or
//!   `no-unlink`, set by the system or a user.
//!
//! Who the caller is, and whether they are an administrator, is decided before
//! this from the connection, never from the request. Whether anything uses the
//! item is asked right before the move, where only `No` passes. Unknown is
//! never eligible. Pure and IO-free.

use std::collections::BTreeSet;

use lumen_domain::{FileIdentity, RawPath};
use serde::{Deserialize, Serialize};

use crate::uninstall::{BundleEvidence, Ineligible, gate};

/// The only folder the helper takes apps from (ADR-0063 rule 2).
pub const APPLICATIONS: &[u8] = b"/Applications";

/// Why the helper does not move an item; it stays where it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum Reason {
    /// It is not the item the person confirmed: gone, replaced, or at
    /// another path.
    Changed,
    /// The account asking owns it, so Lumen moves it itself (ADR-0062).
    OwnedByCaller,
    /// A flag protects it (SIP's `restricted`, or `immutable`,
    /// `append-only` or `no-unlink`).
    Restricted,
    /// Whether a flag protects it could not be read.
    RestrictedUnknown,
    /// Something uses it, or that could not be known for every process.
    /// With full rights, only `No` passes; ADR-0030's partial answer never
    /// applies (ADR-0063 rule 5).
    InUse,
    /// An ADR-0062 check failed or could not be answered.
    Gate(Ineligible),
}

/// Fresh evidence the helper gathered itself about the item a request names.
/// `None` means the check could not be answered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HelperEvidence {
    /// ADR-0062's evidence about the bundle at the requested path. Its
    /// `folders` and `owned_by_user` are not used: the folder is
    /// [`APPLICATIONS`], and the owner is [`Self::owner`].
    pub bundle: BundleEvidence,
    /// Its identity, observed without following links.
    pub identity: Option<FileIdentity>,
    /// The user ID owning the bundle directory.
    pub owner: Option<u32>,
    /// It carries a protecting flag.
    pub restricted: Option<bool>,
}

/// `/Applications` as a path.
fn applications() -> Option<RawPath> {
    RawPath::from_unix_bytes(APPLICATIONS.to_vec()).ok()
}

/// Whether the helper may move the item for `caller` (a user ID from the
/// connection), confirmed as `path` and `identity`, given what it observed.
///
/// # Errors
///
/// Every reason that applies, at least one, each once, in a fixed order.
pub fn decide(
    caller: u32,
    path: &RawPath,
    identity: &FileIdentity,
    evidence: &HelperEvidence,
) -> Result<(), Vec<Reason>> {
    let mut reasons = BTreeSet::new();
    if &evidence.bundle.path != path || evidence.identity.as_ref() != Some(identity) {
        reasons.insert(Reason::Changed);
    }
    match evidence.owner {
        Some(owner) if owner == caller => {
            reasons.insert(Reason::OwnedByCaller);
        }
        Some(_) => {}
        None => {
            reasons.insert(Reason::Gate(Ineligible::OwnerUnknown));
        }
    }
    match evidence.restricted {
        Some(false) => {}
        Some(true) => {
            reasons.insert(Reason::Restricted);
        }
        None => {
            reasons.insert(Reason::RestrictedUnknown);
        }
    }
    // Every ADR-0062 check except the owner check, decided above.
    let shared = BundleEvidence {
        folders: applications().into_iter().collect(),
        owned_by_user: Some(true),
        ..evidence.bundle.clone()
    };
    if let Err(refusal) = gate(&shared) {
        reasons.extend(refusal.reasons().iter().copied().map(Reason::Gate));
    }
    if reasons.is_empty() {
        Ok(())
    } else {
        Err(reasons.into_iter().collect())
    }
}

#[cfg(test)]
mod tests {
    use lumen_domain::{FileId, VolumeId};

    use super::*;
    use crate::uninstall::HelperFolder;

    const CALLER: u32 = 501;

    /// A change to eligible evidence.
    type Change = fn(&mut HelperEvidence);

    fn raw(path: &str) -> RawPath {
        RawPath::from_unix_bytes(path.as_bytes().to_vec()).unwrap_or_else(|_| unreachable!())
    }

    fn identity(file: u128) -> FileIdentity {
        FileIdentity {
            volume: VolumeId::new("dev:1000012").unwrap_or_else(|_| unreachable!()),
            file: FileId::new(file),
        }
    }

    fn eligible(path: &str) -> HelperEvidence {
        HelperEvidence {
            bundle: BundleEvidence {
                path: raw(path),
                folders: Vec::new(),
                directory: Some(true),
                owned_by_user: Some(false),
                linked_services: Some(0),
                helpers: HelperFolder::ALL.map(|f| (f, Some(false))).into(),
                running: Some(false),
                store_on_volume: Some(true),
                security_software: Some(false),
                managed: Some(false),
            },
            identity: Some(identity(7)),
            owner: Some(0),
            restricted: Some(false),
        }
    }

    fn reasons(path: &str, evidence: &HelperEvidence) -> Vec<Reason> {
        decide(CALLER, &raw(path), &identity(7), evidence)
            .err()
            .unwrap_or_default()
    }

    #[test]
    fn a_quiet_app_another_account_owns_in_applications_is_admitted() {
        let path = "/Applications/Word.app";
        assert_eq!(
            decide(CALLER, &raw(path), &identity(7), &eligible(path)),
            Ok(())
        );
        // Any other owner, not only root (another person's drag-installed app).
        let mut other = eligible(path);
        other.owner = Some(502);
        assert_eq!(decide(CALLER, &raw(path), &identity(7), &other), Ok(()));
    }

    #[test]
    fn the_folders_and_owner_in_the_evidence_are_ignored() {
        // A home folder is never a helper folder, whatever the evidence lists.
        let path = "/Users/ana/Applications/Word.app";
        let mut home = eligible(path);
        home.bundle.folders = vec![raw("/Users/ana/Applications")];
        assert_eq!(
            reasons(path, &home),
            [Reason::Gate(Ineligible::NotInApplications)]
        );
        // `owned_by_user` is not how the helper learns the owner.
        let path = "/Applications/Word.app";
        let mut claims = eligible(path);
        claims.bundle.owned_by_user = Some(true);
        claims.owner = Some(CALLER);
        assert_eq!(reasons(path, &claims), [Reason::OwnedByCaller]);
    }

    #[test]
    fn only_the_confirmed_item_at_the_confirmed_path() {
        let path = "/Applications/Word.app";
        let mut swapped = eligible(path);
        swapped.identity = Some(identity(8));
        assert_eq!(reasons(path, &swapped), [Reason::Changed]);
        let mut gone = eligible(path);
        gone.identity = None;
        assert_eq!(reasons(path, &gone), [Reason::Changed]);
        let elsewhere = eligible("/Applications/Excel.app");
        assert_eq!(reasons(path, &elsewhere), [Reason::Changed]);
    }

    #[test]
    fn protected_and_nested_locations_are_refused() {
        for path in [
            "/System/Applications/Calculator.app",
            "/Library/Apple/Thing.app",
            "/Applications/Utilities/Tool.app",
            "/Applications/../System/Applications/Calculator.app",
            "/Applications/Word",
            "/Applications",
            "Applications/Word.app",
        ] {
            let found = reasons(path, &eligible(path));
            assert!(
                found.contains(&Reason::Gate(Ineligible::NotInApplications)),
                "{path}: {found:?}"
            );
        }
    }

    #[test]
    fn every_failed_or_unknown_check_refuses_with_its_reason() {
        let path = "/Applications/Word.app";
        let cases: Vec<(Change, Reason)> = vec![
            (|e| e.owner = Some(CALLER), Reason::OwnedByCaller),
            (|e| e.owner = None, Reason::Gate(Ineligible::OwnerUnknown)),
            (|e| e.restricted = Some(true), Reason::Restricted),
            (|e| e.restricted = None, Reason::RestrictedUnknown),
            (
                |e| e.bundle.directory = Some(false),
                Reason::Gate(Ineligible::NotABundle),
            ),
            (
                |e| e.bundle.directory = None,
                Reason::Gate(Ineligible::NotABundle),
            ),
            (
                |e| e.bundle.linked_services = Some(1),
                Reason::Gate(Ineligible::ServiceLinked),
            ),
            (
                |e| e.bundle.linked_services = None,
                Reason::Gate(Ineligible::ServicesUnknown),
            ),
            (
                |e| {
                    e.bundle
                        .helpers
                        .insert(HelperFolder::LaunchDaemons, Some(true));
                },
                Reason::Gate(Ineligible::HasHelpers),
            ),
            (
                |e| {
                    e.bundle.helpers.remove(&HelperFolder::SystemExtensions);
                },
                Reason::Gate(Ineligible::HelpersUnknown),
            ),
            (
                |e| e.bundle.running = Some(true),
                Reason::Gate(Ineligible::Running),
            ),
            (
                |e| e.bundle.running = None,
                Reason::Gate(Ineligible::RunningUnknown),
            ),
            (
                |e| e.bundle.store_on_volume = Some(false),
                Reason::Gate(Ineligible::NoStoreOnVolume),
            ),
            (
                |e| e.bundle.store_on_volume = None,
                Reason::Gate(Ineligible::StoreUnknown),
            ),
        ];
        for (change, reason) in cases {
            let mut evidence = eligible(path);
            change(&mut evidence);
            assert_eq!(reasons(path, &evidence), [reason], "{reason:?}");
        }
    }

    #[test]
    fn all_reasons_are_reported_once_in_a_fixed_order() {
        let path = "/Applications/Word.app";
        let mut evidence = eligible(path);
        evidence.identity = None;
        evidence.owner = None;
        evidence.restricted = None;
        evidence.bundle.running = None;
        evidence.bundle.directory = None;
        assert_eq!(
            reasons(path, &evidence),
            [
                Reason::Changed,
                Reason::RestrictedUnknown,
                Reason::Gate(Ineligible::NotABundle),
                Reason::Gate(Ineligible::OwnerUnknown),
                Reason::Gate(Ineligible::RunningUnknown),
            ]
        );
    }

    #[test]
    fn reasons_have_a_stable_wire_form() {
        let json =
            serde_json::to_string(&[Reason::OwnedByCaller, Reason::Gate(Ineligible::Running)])
                .unwrap_or_default();
        assert_eq!(json, r#"["owned_by_caller",{"gate":"running"}]"#);
    }
}
