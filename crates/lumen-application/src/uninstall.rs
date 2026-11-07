//! Uninstalling a user-owned macOS application by moving it into quarantine
//! (ADR-0062).
//!
//! The cleanup policy is not asked: the person chose to uninstall, and
//! [`gate`] is this action's own deterministic gate. It runs on fresh,
//! read-only evidence ([`BundleEvidence`]) when the preview is made and again
//! right before the move. Every check that fails, or cannot be answered,
//! refuses with its reason ([`Ineligible`]); unknown is never eligible.
//!
//! A preview is stored as a one-item cleanup plan chosen by the person, so it
//! is confirmed by its plan hash (ADR-0034), journaled, listed in Quarantine,
//! restored and finalized exactly like any other item. Its stored decision is
//! [`decision`]: rule [`RULE`], never one the policy fires, which is how an
//! uninstall plan and a cleanup plan tell each other apart and refuse each
//! other ([`is_uninstall`]). The decision's evidence hash binds the facts the
//! person saw ([`Binding`]), so an app that changed after its preview is
//! refused. Pure and IO-free.

use std::collections::{BTreeMap, BTreeSet};

use lumen_domain::{
    EvidenceHash, FileIdentity, FiredRule, JevEffect, PathFlavor, PolicyDecision, PolicyStage,
    PolicyVersion, RawPath, ReclaimEstimate, Risk, RiskLevel, RuleId, UntrustedText, Verdict,
};

/// The rule an uninstall's decision cites: the gate passed and the person
/// chose to uninstall. The policy never fires it.
pub const RULE: &str = "uninstall.macos-app";

/// The version of [`gate`], recorded as the decision's policy version.
pub const GATE_VERSION: PolicyVersion = PolicyVersion::new(1, 0, 0);

/// Domain separation for [`Binding::hash`].
const HASH_DOMAIN: &[u8] = b"lumen.uninstall/1\0";

/// A folder inside a bundle that holds helpers registered with the system
/// (`Contents/Library/<name>`). Moving only the app would leave them behind
/// and broken, so a bundle with any of them is not uninstalled this way.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum HelperFolder {
    /// `LoginItems`.
    LoginItems,
    /// `LaunchServices` (privileged helper tools).
    LaunchServices,
    /// `LaunchAgents`.
    LaunchAgents,
    /// `LaunchDaemons`.
    LaunchDaemons,
    /// `SystemExtensions`.
    SystemExtensions,
}

impl HelperFolder {
    /// Every helper folder.
    pub const ALL: [Self; 5] = [
        Self::LoginItems,
        Self::LaunchServices,
        Self::LaunchAgents,
        Self::LaunchDaemons,
        Self::SystemExtensions,
    ];

    /// Its name inside `Contents/Library`.
    pub const fn name(self) -> &'static str {
        match self {
            Self::LoginItems => "LoginItems",
            Self::LaunchServices => "LaunchServices",
            Self::LaunchAgents => "LaunchAgents",
            Self::LaunchDaemons => "LaunchDaemons",
            Self::SystemExtensions => "SystemExtensions",
        }
    }
}

/// Why an application cannot be uninstalled into quarantine. Each "unknown"
/// reason is a check that could not be answered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Ineligible {
    /// It is not a `.app` directly in `/Applications` or `~/Applications`.
    NotInApplications,
    /// It is under `/System` or `/Library/Apple`.
    Protected,
    /// It is not a directory (a link or a file), or could not be observed.
    NotABundle,
    /// Who owns the bundle could not be read.
    OwnerUnknown,
    /// Another account owns it: an administrator installer or the App Store
    /// installed it.
    NotOwnedByUser,
    /// The background items could not all be read, so a linked one may be
    /// missing.
    ServicesUnknown,
    /// A background item is linked to it (verified or heuristic).
    ServiceLinked,
    /// Whether it has helper folders could not be answered.
    HelpersUnknown,
    /// It has a helper folder ([`HelperFolder`]).
    HasHelpers,
    /// Whether it runs could not be known.
    RunningUnknown,
    /// It runs.
    Running,
    /// Whether its volume has a quarantine store could not be known.
    StoreUnknown,
    /// Its volume has no quarantine store, so it cannot be moved by renaming.
    NoStoreOnVolume,
}

/// Fresh, read-only evidence about one bundle, gathered right before the
/// gate runs. `None` means the check could not be answered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BundleEvidence {
    /// The bundle, as the application inventory lists it.
    pub path: RawPath,
    /// The folders an uninstallable bundle may lie directly in
    /// (`/Applications`, `~/Applications`).
    pub folders: Vec<RawPath>,
    /// It is a directory, observed without following links.
    pub directory: Option<bool>,
    /// The account Lumen runs as owns the bundle directory.
    pub owned_by_user: Option<bool>,
    /// How many background items are linked to it, by any method; `None` if
    /// the background items were not read completely.
    pub linked_services: Option<usize>,
    /// Whether each helper folder exists; a folder missing from the map was
    /// not checked.
    pub helpers: BTreeMap<HelperFolder, Option<bool>>,
    /// It runs.
    pub running: Option<bool>,
    /// Its volume has a quarantine store.
    pub store_on_volume: Option<bool>,
}

/// Why the gate refused: at least one reason, in a fixed order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal(Vec<Ineligible>);

impl Refusal {
    /// The reasons, at least one, each once, in a fixed order.
    pub fn reasons(&self) -> &[Ineligible] {
        &self.0
    }
}

/// Splits a Unix path into its components; `None` for a Windows path, a
/// relative path, or one with `.`, `..` or empty components.
fn components(path: &RawPath) -> Option<Vec<&[u8]>> {
    if path.flavor() != PathFlavor::Unix {
        return None;
    }
    let rest = path.as_bytes().strip_prefix(b"/")?;
    let rest = rest.strip_suffix(b"/").unwrap_or(rest);
    if rest.is_empty() {
        return Some(Vec::new());
    }
    let parts: Vec<&[u8]> = rest.split(|&b| b == b'/').collect();
    parts
        .iter()
        .all(|part| !matches!(*part, b"" | b"." | b".."))
        .then_some(parts)
}

/// Whether `path` is a `.app` directly inside one of `folders`, compared
/// component by component.
fn directly_in(path: &RawPath, folders: &[RawPath]) -> bool {
    let Some(parts) = components(path) else {
        return false;
    };
    let Some((leaf, parent)) = parts.split_last() else {
        return false;
    };
    let bundle = leaf.len() > 4
        && leaf
            .get(leaf.len() - 4..)
            .is_some_and(|ext| ext.eq_ignore_ascii_case(b".app"));
    bundle
        && folders
            .iter()
            .filter_map(components)
            .any(|folder| !folder.is_empty() && folder == parent)
}

/// Whether `path` lies under `/System` or `/Library/Apple`. A path that cannot
/// be split counts as protected.
fn protected(path: &RawPath) -> bool {
    components(path).is_none_or(|parts| {
        parts.first() == Some(&&b"System"[..])
            || (parts.first() == Some(&&b"Library"[..]) && parts.get(1) == Some(&&b"Apple"[..]))
    })
}

/// ADR-0062's gate: `Ok` only if every check passed with a known answer.
///
/// # Errors
///
/// A [`Refusal`] with every reason that applies.
pub fn gate(evidence: &BundleEvidence) -> Result<(), Refusal> {
    let mut reasons = BTreeSet::new();
    let mut refuse = |reason| {
        reasons.insert(reason);
    };
    if !directly_in(&evidence.path, &evidence.folders) {
        refuse(Ineligible::NotInApplications);
    }
    if protected(&evidence.path) {
        refuse(Ineligible::Protected);
    }
    if evidence.directory != Some(true) {
        refuse(Ineligible::NotABundle);
    }
    match evidence.owned_by_user {
        Some(true) => {}
        Some(false) => refuse(Ineligible::NotOwnedByUser),
        None => refuse(Ineligible::OwnerUnknown),
    }
    match evidence.linked_services {
        Some(0) => {}
        Some(_) => refuse(Ineligible::ServiceLinked),
        None => refuse(Ineligible::ServicesUnknown),
    }
    for folder in HelperFolder::ALL {
        match evidence.helpers.get(&folder).copied().flatten() {
            Some(false) => {}
            Some(true) => refuse(Ineligible::HasHelpers),
            None => refuse(Ineligible::HelpersUnknown),
        }
    }
    match evidence.running {
        Some(false) => {}
        Some(true) => refuse(Ineligible::Running),
        None => refuse(Ineligible::RunningUnknown),
    }
    match evidence.store_on_volume {
        Some(true) => {}
        Some(false) => refuse(Ineligible::NoStoreOnVolume),
        None => refuse(Ineligible::StoreUnknown),
    }
    if reasons.is_empty() {
        Ok(())
    } else {
        Err(Refusal(reasons.into_iter().collect()))
    }
}

/// What a person confirms about the app, besides the plan's own fields: the
/// facts its preview showed. Any change between preview and move changes
/// [`Binding::hash`], and the move is refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Binding<'a> {
    /// The software list's key (`MacApp:<bundle id>`).
    pub key: &'a str,
    /// The bundle identifier.
    pub bundle_id: &'a UntrustedText,
    /// The display name.
    pub name: Option<&'a UntrustedText>,
    /// The version.
    pub version: Option<&'a UntrustedText>,
    /// Where the bundle is.
    pub path: &'a RawPath,
    /// What it is.
    pub identity: &'a FileIdentity,
    /// What it frees.
    pub reclaim: ReclaimEstimate,
    /// Every folder inside it was listed.
    pub size_complete: bool,
}

impl Binding<'_> {
    /// BLAKE3 over `lumen.uninstall/1\0` and the canonical JSON of the facts.
    ///
    /// # Panics
    ///
    /// Never in practice: these types serialize infallibly.
    pub fn hash(&self) -> EvidenceHash {
        let value = serde_json::json!({
            "key": self.key,
            "bundle_id": self.bundle_id,
            "name": self.name,
            "version": self.version,
            "path": self.path,
            "identity": self.identity,
            "reclaim": self.reclaim,
            "size_complete": self.size_complete,
        });
        let bytes = serde_json::to_vec(&value)
            .unwrap_or_else(|e| unreachable!("binding serialization is infallible: {e}"));
        let mut hasher = blake3::Hasher::new();
        hasher.update(HASH_DOMAIN);
        hasher.update(&bytes);
        EvidenceHash::from_bytes(*hasher.finalize().as_bytes())
    }
}

/// The decision an uninstall plan stores and its manifest records: `REVIEW`
/// (the person chose it, ADR-0051), rule [`RULE`], over `evidence`.
///
/// # Panics
///
/// Never: the rule ID is valid and the decision cites one non-protection
/// rule without Jev.
pub fn decision(evidence: EvidenceHash) -> PolicyDecision {
    let rule = RuleId::new(RULE).unwrap_or_else(|_| unreachable!("valid rule ID"));
    PolicyDecision::new(
        Verdict::Review,
        GATE_VERSION,
        evidence,
        [FiredRule {
            rule,
            stage: PolicyStage::Eligibility,
            evidence: BTreeSet::new(),
        }],
        Risk {
            level: RiskLevel::Medium,
            factors: BTreeSet::new(),
        },
        JevEffect::None,
    )
    .unwrap_or_else(|e| unreachable!("a valid decision: {e}"))
}

/// Whether `decisions` are an uninstall plan's: exactly one, citing
/// [`RULE`].
pub fn is_uninstall(decisions: &[PolicyDecision]) -> bool {
    matches!(decisions, [only] if cites_rule(only))
}

/// Whether any of `decisions` cites [`RULE`]: such a plan is not a cleanup
/// plan, whatever else it holds.
pub fn any_uninstall(decisions: &[PolicyDecision]) -> bool {
    decisions.iter().any(cites_rule)
}

fn cites_rule(decision: &PolicyDecision) -> bool {
    decision
        .fired_rules()
        .iter()
        .any(|fired| fired.rule.as_str() == RULE)
}

#[cfg(test)]
mod tests {
    use lumen_domain::{FileId, VolumeId};

    use super::*;

    fn raw(path: &str) -> RawPath {
        RawPath::from_unix_bytes(path.as_bytes().to_vec()).unwrap_or_else(|_| unreachable!())
    }

    fn eligible(path: &str) -> BundleEvidence {
        BundleEvidence {
            path: raw(path),
            folders: vec![raw("/Applications"), raw("/Users/ana/Applications")],
            directory: Some(true),
            owned_by_user: Some(true),
            linked_services: Some(0),
            helpers: HelperFolder::ALL.map(|f| (f, Some(false))).into(),
            running: Some(false),
            store_on_volume: Some(true),
        }
    }

    fn refused(evidence: &BundleEvidence) -> Vec<Ineligible> {
        gate(evidence)
            .err()
            .map(|r| r.reasons().to_vec())
            .unwrap_or_default()
    }

    #[test]
    fn a_users_own_quiet_app_in_either_folder_is_eligible() {
        assert_eq!(gate(&eligible("/Applications/Zed.app")), Ok(()));
        assert_eq!(gate(&eligible("/Users/ana/Applications/Zed.APP")), Ok(()));
    }

    #[test]
    fn only_a_bundle_directly_in_an_applications_folder() {
        for path in [
            "/Applications/Utilities/Tool.app",
            "/Applications/Zed",
            "/Applications/.app",
            "/Applications/../Applications/Zed.app",
            "/Applications/./Zed.app",
            "/Applications//Zed.app",
            "/Users/ana/Desktop/Zed.app",
            "/Applications.app",
            "Applications/Zed.app",
        ] {
            let reasons = refused(&eligible(path));
            assert!(reasons.contains(&Ineligible::NotInApplications), "{path}");
            // A path that cannot be compared is treated as protected too.
            assert!(
                reasons.len() == 1 || reasons[1] == Ineligible::Protected,
                "{path}"
            );
        }
        let mut no_folders = eligible("/Applications/Zed.app");
        no_folders.folders = vec![raw("/")];
        assert_eq!(refused(&no_folders), [Ineligible::NotInApplications]);
    }

    #[test]
    fn system_locations_are_protected_even_if_listed_as_a_folder() {
        for (path, folder) in [
            (
                "/System/Applications/Calculator.app",
                "/System/Applications",
            ),
            ("/Library/Apple/Safari.app", "/Library/Apple"),
        ] {
            let mut evidence = eligible(path);
            evidence.folders.push(raw(folder));
            assert_eq!(refused(&evidence), [Ineligible::Protected], "{path}");
        }
        let windows = RawPath::from_windows_wide(
            &"C:\\Program Files\\Zed.app"
                .encode_utf16()
                .collect::<Vec<_>>(),
        )
        .unwrap_or_else(|_| unreachable!());
        let mut evidence = eligible("/Applications/Zed.app");
        evidence.path = windows;
        assert_eq!(
            refused(&evidence),
            [Ineligible::NotInApplications, Ineligible::Protected]
        );
    }

    #[test]
    fn every_failed_or_unknown_check_refuses_with_its_reason() {
        type Change = fn(&mut BundleEvidence);
        let cases: [(Change, Ineligible); 12] = [
            (|e| e.directory = Some(false), Ineligible::NotABundle),
            (|e| e.directory = None, Ineligible::NotABundle),
            (
                |e| e.owned_by_user = Some(false),
                Ineligible::NotOwnedByUser,
            ),
            (|e| e.owned_by_user = None, Ineligible::OwnerUnknown),
            (|e| e.linked_services = Some(1), Ineligible::ServiceLinked),
            (|e| e.linked_services = None, Ineligible::ServicesUnknown),
            (
                |e| {
                    e.helpers.insert(HelperFolder::LoginItems, Some(true));
                },
                Ineligible::HasHelpers,
            ),
            (
                |e| {
                    e.helpers.insert(HelperFolder::SystemExtensions, None);
                },
                Ineligible::HelpersUnknown,
            ),
            (
                |e| {
                    e.helpers.remove(&HelperFolder::LaunchDaemons);
                },
                Ineligible::HelpersUnknown,
            ),
            (|e| e.running = Some(true), Ineligible::Running),
            (|e| e.running = None, Ineligible::RunningUnknown),
            (|e| e.store_on_volume = None, Ineligible::StoreUnknown),
        ];
        for (change, reason) in cases {
            let mut evidence = eligible("/Applications/Zed.app");
            change(&mut evidence);
            assert_eq!(refused(&evidence), [reason]);
        }
        let mut evidence = eligible("/Applications/Zed.app");
        evidence.store_on_volume = Some(false);
        assert_eq!(refused(&evidence), [Ineligible::NoStoreOnVolume]);
    }

    #[test]
    fn all_reasons_are_reported_once_in_a_fixed_order() {
        let evidence = BundleEvidence {
            path: raw("/System/Zed.app"),
            folders: Vec::new(),
            directory: None,
            owned_by_user: None,
            linked_services: None,
            helpers: BTreeMap::new(),
            running: None,
            store_on_volume: None,
        };
        assert_eq!(
            refused(&evidence),
            [
                Ineligible::NotInApplications,
                Ineligible::Protected,
                Ineligible::NotABundle,
                Ineligible::OwnerUnknown,
                Ineligible::ServicesUnknown,
                Ineligible::HelpersUnknown,
                Ineligible::RunningUnknown,
                Ineligible::StoreUnknown,
            ]
        );
    }

    fn identity(file: u128) -> FileIdentity {
        FileIdentity {
            volume: VolumeId::new("dev:1").unwrap_or_else(|_| unreachable!()),
            file: FileId::new(file),
        }
    }

    #[test]
    fn the_binding_hash_changes_with_every_fact() {
        let id = UntrustedText::new("dev.zed.Zed");
        let other_id = UntrustedText::new("dev.zed.Zed2");
        let name = UntrustedText::new("Zed");
        let path = raw("/Applications/Zed.app");
        let other_path = raw("/Applications/Zed2.app");
        let (one, two) = (identity(1), identity(2));
        let base = Binding {
            key: "MacApp:dev.zed.Zed",
            bundle_id: &id,
            name: Some(&name),
            version: None,
            path: &path,
            identity: &one,
            reclaim: ReclaimEstimate::default(),
            size_complete: true,
        };
        assert_eq!(base.hash(), base.clone().hash(), "deterministic");
        let changed = [
            Binding {
                key: "MacApp:other",
                ..base.clone()
            },
            Binding {
                bundle_id: &other_id,
                ..base.clone()
            },
            Binding {
                name: None,
                ..base.clone()
            },
            Binding {
                version: Some(&name),
                ..base.clone()
            },
            Binding {
                path: &other_path,
                ..base.clone()
            },
            Binding {
                identity: &two,
                ..base.clone()
            },
            Binding {
                reclaim: ReclaimEstimate {
                    files: 1,
                    ..ReclaimEstimate::default()
                },
                ..base.clone()
            },
            Binding {
                size_complete: false,
                ..base.clone()
            },
        ];
        for other in changed {
            assert_ne!(other.hash(), base.hash(), "{other:?}");
        }
    }

    #[test]
    fn an_uninstall_decision_is_recognised_and_never_a_policy_rule() {
        let decision = decision(EvidenceHash::from_bytes([3; 32]));
        assert_eq!(decision.verdict(), Verdict::Review);
        assert_eq!(decision.policy_version(), GATE_VERSION);
        assert!(is_uninstall(std::slice::from_ref(&decision)));
        assert!(!is_uninstall(&[decision.clone(), decision.clone()]));
        assert!(any_uninstall(&[decision.clone(), decision]));
        assert!(!is_uninstall(&[]));
    }
}
