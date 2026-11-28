//! Whether Lumen may remove one Homebrew formula or cask through Homebrew,
//! and the exact command (ADR-0060, roadmap 16.7, #809). Pure and IO-free.
//!
//! Homebrew keeps its own records of what it installed, so its packages are
//! removed by `brew uninstall`, never moved into quarantine (ADR-0062: the
//! manager would still believe they are installed). Lumen cannot undo that
//! removal; the preview says so (ADR-0060 rule 5).
//!
//! The command is built from the package's identity, never copied from a
//! record or the page (ADR-0060 rule 3, as ADR-0049 runs a tool's command):
//! `brew uninstall --formula <name>` or `brew uninstall --cask <token>`, with
//! no `--zap` (it would also remove the app's data in `~/Library`), no
//! `--force` and no `--ignore-dependencies`. A name that is not a plain
//! Homebrew name is refused, so it can never read as an option.
//!
//! Every check below must pass; each that fails or cannot be answered is a
//! reason, and unknown is never "removable":
//!
//! - exactly one listed package of that identity, from an inventory read
//!   whole;
//! - not protected (ADR-0060 rule 4);
//! - no listed package's record says it needs it, and every record that could
//!   was read (the list's [`RemovalFacts::needed_by`], #914): Homebrew would
//!   refuse anyway, and Lumen never removes what others rely on;
//! - it runs with the person's own rights: no step needs administrator rights
//!   (ADR-0060 rule 6). Lumen's helper does not run package managers;
//! - not security software and not managed by an organization (ADR-0060
//!   rule 4, owner decision 2026-11-16);
//! - nothing runs from it: no process's program lies inside its folder or an
//!   app it placed, so no open work is lost (as ADR-0062 refuses a running
//!   app).
//!
//! A preview that passes is stored as a one-item plan chosen by the person,
//! confirmed by its hash (ADR-0034); the page sends only its ID (ADR-0023).
//! Its decision cites [`RULE`], which the policy never fires, so neither a
//! clean-up nor an app's uninstall ever runs it, and it never runs as one
//! ([`is_removal`], [`any_removal`]). Its evidence hash binds what the person
//! saw ([`Binding`]): the package, its folder, the program and the command.

use std::collections::BTreeSet;

use lumen_domain::{
    EvidenceHash, FileIdentity, FiredRule, JevEffect, PackageId, PackageManager, PolicyDecision,
    PolicyStage, PolicyVersion, RawPath, RemovalFacts, Risk, RiskLevel, RuleId, UntrustedText,
    Verdict,
};

/// The rule a Homebrew removal preview's decision cites: the gate passed and
/// the person chose to remove the package. The policy never fires it.
pub const RULE: &str = "homebrew.uninstall";

/// The version of [`homebrew_removal`], recorded as the decision's policy
/// version.
pub const GATE_VERSION: PolicyVersion = PolicyVersion::new(1, 0, 0);

/// Domain separation for [`Binding::hash`].
const HASH_DOMAIN: &[u8] = b"lumen.homebrew-uninstall/1\0";

/// The program Lumen runs, found as ADR-0049 finds a tool.
pub const PROGRAM: &str = "brew";

/// What Lumen knows about one Homebrew package right before it decides.
/// `None` means the check could not be answered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HomebrewEvidence<'a> {
    /// The package the person chose, by manager and name.
    pub package: &'a PackageId,
    /// How many listed packages have that identity.
    pub listed: usize,
    /// The Homebrew inventory was read whole.
    pub inventory_complete: bool,
    /// The list's facts about it.
    pub facts: Option<&'a RemovalFacts>,
    /// Removing it needs no administrator rights: every step Homebrew's
    /// record says it takes runs with the person's rights.
    pub user_rights_only: Option<bool>,
    /// It is security software (for a cask, an app it installed carries the
    /// Endpoint Security entitlement).
    pub security_software: Option<bool>,
    /// An organization may manage it.
    pub managed: Option<bool>,
    /// A process runs a program inside its folder or an app it placed.
    pub running: Option<bool>,
}

/// Why Lumen does not remove it; it stays as it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum Blocked {
    /// It is not a Homebrew formula or cask.
    NotHomebrew,
    /// Its name is not a plain Homebrew name.
    UnsafeName,
    /// No listed package has that identity now.
    NotListed,
    /// More than one does.
    Ambiguous,
    /// The Homebrew inventory was not read whole.
    InventoryIncomplete,
    /// It is protected or part of the system.
    Protected,
    /// Another listed package's record says it needs it.
    Dependents,
    /// Whether anything needs it could not be told.
    DependentsUnknown,
    /// Removing it needs administrator rights.
    NeedsAdministrator,
    /// Whether removing it needs administrator rights could not be told.
    AdministratorUnknown,
    /// It is security software.
    SecuritySoftware,
    /// Whether it is security software could not be told.
    SecurityUnknown,
    /// An organization may manage it.
    Managed,
    /// Whether an organization manages it could not be told.
    ManagedUnknown,
    /// Something it installed is running: quit it first.
    Running,
    /// Whether something it installed is running could not be told.
    RunningUnknown,
}

/// The exact command: [`PROGRAM`] and its arguments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HomebrewCommand {
    /// The arguments, in order.
    pub args: Vec<String>,
}

/// Whether `name` is a plain Homebrew formula or cask name: it starts with a
/// lowercase letter or digit, and holds only those, `@`, `.`, `_`, `+` and
/// `-`, at most 128 bytes. A tap-qualified name or anything that could read
/// as an option or a path is not.
fn plain(name: &str) -> bool {
    let mut bytes = name.bytes();
    let first = bytes
        .next()
        .is_some_and(|b| b.is_ascii_lowercase() || b.is_ascii_digit());
    first
        && name.len() <= 128
        && bytes.all(|b| {
            b.is_ascii_lowercase()
                || b.is_ascii_digit()
                || matches!(b, b'@' | b'.' | b'_' | b'+' | b'-')
        })
}

/// Records `reason` unless the answer is `Some(false)`.
fn require_not(answer: Option<bool>, yes: Blocked, unknown: Blocked, reasons: &mut Vec<Blocked>) {
    match answer {
        Some(false) => {}
        Some(true) => reasons.push(yes),
        None => reasons.push(unknown),
    }
}

/// The command that removes the package `evidence` describes.
///
/// # Errors
///
/// Every reason it is not removed, at least one, each once, in a fixed
/// order.
pub fn homebrew_removal(evidence: &HomebrewEvidence<'_>) -> Result<HomebrewCommand, Vec<Blocked>> {
    let mut reasons = Vec::new();
    let kind = match evidence.package.manager {
        PackageManager::Homebrew => Some("--formula"),
        PackageManager::HomebrewCask => Some("--cask"),
        _ => None,
    };
    if kind.is_none() {
        reasons.push(Blocked::NotHomebrew);
    }
    let name = evidence.package.id.raw();
    if !plain(name) {
        reasons.push(Blocked::UnsafeName);
    }
    match evidence.listed {
        0 => reasons.push(Blocked::NotListed),
        1 => {}
        _ => reasons.push(Blocked::Ambiguous),
    }
    if !evidence.inventory_complete {
        reasons.push(Blocked::InventoryIncomplete);
    }
    match evidence.facts {
        Some(facts) => {
            if facts.protected {
                reasons.push(Blocked::Protected);
            }
            match facts.needed_by {
                Some(count) if count.any() => reasons.push(Blocked::Dependents),
                Some(count) if count.complete => {}
                _ => reasons.push(Blocked::DependentsUnknown),
            }
        }
        None => reasons.push(Blocked::DependentsUnknown),
    }
    require_not(
        evidence.user_rights_only.map(|only| !only),
        Blocked::NeedsAdministrator,
        Blocked::AdministratorUnknown,
        &mut reasons,
    );
    require_not(
        evidence.security_software,
        Blocked::SecuritySoftware,
        Blocked::SecurityUnknown,
        &mut reasons,
    );
    require_not(
        evidence.managed,
        Blocked::Managed,
        Blocked::ManagedUnknown,
        &mut reasons,
    );
    require_not(
        evidence.running,
        Blocked::Running,
        Blocked::RunningUnknown,
        &mut reasons,
    );
    match kind {
        Some(kind) if reasons.is_empty() => Ok(HomebrewCommand {
            args: vec!["uninstall".to_owned(), kind.to_owned(), name.to_owned()],
        }),
        _ => {
            reasons.sort_unstable();
            reasons.dedup();
            Err(reasons)
        }
    }
}

/// The facts a stored preview binds: what the person confirmed, and what
/// must still hold right before the command runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Binding<'a> {
    /// The software list's key.
    pub key: &'a str,
    /// The package.
    pub package: &'a PackageId,
    /// Its version, as listed.
    pub version: Option<&'a UntrustedText>,
    /// Its folder in Homebrew's prefix (`Cellar/<name>` or
    /// `Caskroom/<token>`).
    pub folder: &'a RawPath,
    /// What that folder is.
    pub identity: &'a FileIdentity,
    /// The program run, by absolute path, in the same prefix.
    pub program: &'a RawPath,
    /// The command.
    pub command: &'a HomebrewCommand,
    /// The apps a cask placed, which go with it.
    pub apps: &'a [RawPath],
}

impl Binding<'_> {
    /// BLAKE3 over `lumen.homebrew-uninstall/1\0` and the canonical JSON of
    /// the facts.
    ///
    /// # Panics
    ///
    /// Never in practice: these types serialize infallibly.
    pub fn hash(&self) -> EvidenceHash {
        let value = serde_json::json!({
            "key": self.key,
            "package": self.package,
            "version": self.version,
            "folder": self.folder,
            "identity": self.identity,
            "program": self.program,
            "args": self.command.args,
            "apps": self.apps,
        });
        let bytes = serde_json::to_vec(&value)
            .unwrap_or_else(|e| unreachable!("binding serialization is infallible: {e}"));
        let mut hasher = blake3::Hasher::new();
        hasher.update(HASH_DOMAIN);
        hasher.update(&bytes);
        EvidenceHash::from_bytes(*hasher.finalize().as_bytes())
    }
}

/// The decision a removal preview stores: `REVIEW` (the person chose it,
/// ADR-0051), rule [`RULE`], over `evidence`.
///
/// # Panics
///
/// Never: the rule ID and decision are valid by construction.
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
            level: RiskLevel::High,
            factors: BTreeSet::new(),
        },
        JevEffect::None,
    )
    .unwrap_or_else(|e| unreachable!("a valid decision: {e}"))
}

/// Whether `decisions` are a removal preview's: exactly one, citing [`RULE`].
pub fn is_removal(decisions: &[PolicyDecision]) -> bool {
    matches!(decisions, [only] if cites_rule(only))
}

/// Whether any of `decisions` cites [`RULE`]: such a plan is never a
/// clean-up, an app's uninstall or a copy's move, whatever else it holds.
pub fn any_removal(decisions: &[PolicyDecision]) -> bool {
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
    use lumen_domain::{RelationCount, UntrustedText};

    use super::*;

    fn id(manager: PackageManager, name: &str) -> PackageId {
        PackageId {
            manager,
            id: UntrustedText::new(name),
        }
    }

    fn facts(needed_by: Option<RelationCount>) -> RemovalFacts {
        RemovalFacts {
            protected: false,
            mechanism: None,
            installed_by: None,
            needs_administrator: false,
            needs: None,
            needed_by,
        }
    }

    const NONE: RelationCount = RelationCount {
        count: 0,
        complete: true,
    };

    fn removable<'a>(package: &'a PackageId, facts: &'a RemovalFacts) -> HomebrewEvidence<'a> {
        HomebrewEvidence {
            package,
            listed: 1,
            inventory_complete: true,
            facts: Some(facts),
            user_rights_only: Some(true),
            security_software: Some(false),
            managed: Some(false),
            running: Some(false),
        }
    }

    #[test]
    fn a_formula_and_a_cask_get_exactly_their_command_and_never_zap() {
        let none = facts(Some(NONE));
        for (manager, flag) in [
            (PackageManager::Homebrew, "--formula"),
            (PackageManager::HomebrewCask, "--cask"),
        ] {
            let package = id(manager, "act");
            assert_eq!(
                homebrew_removal(&removable(&package, &none)),
                Ok(HomebrewCommand {
                    args: vec!["uninstall".into(), flag.into(), "act".into()]
                })
            );
        }
        let versioned = id(PackageManager::Homebrew, "openssl@3");
        let ok = homebrew_removal(&removable(&versioned, &none));
        assert!(ok.is_ok_and(|c| {
            !c.args
                .iter()
                .any(|a| a.contains("zap") || a.contains("force"))
        }));
    }

    #[test]
    fn a_name_that_could_read_as_an_option_or_a_path_is_refused() {
        let none = facts(Some(NONE));
        for name in [
            "",
            "-rf",
            "--zap",
            "../x",
            "a/b",
            "user/tap/act",
            "Act",
            "a b",
            "a;b",
        ] {
            let package = id(PackageManager::Homebrew, name);
            assert!(
                homebrew_removal(&removable(&package, &none))
                    .is_err_and(|reasons| reasons.contains(&Blocked::UnsafeName)),
                "{name:?}"
            );
        }
    }

    #[test]
    fn every_failed_or_unknown_check_refuses_with_its_reason() {
        type Change = fn(&mut HomebrewEvidence<'_>);
        let cases: [(Change, Blocked); 13] = [
            (|e| e.listed = 0, Blocked::NotListed),
            (|e| e.listed = 2, Blocked::Ambiguous),
            (
                |e| e.inventory_complete = false,
                Blocked::InventoryIncomplete,
            ),
            (|e| e.facts = None, Blocked::DependentsUnknown),
            (
                |e| e.user_rights_only = Some(false),
                Blocked::NeedsAdministrator,
            ),
            (|e| e.user_rights_only = None, Blocked::AdministratorUnknown),
            (
                |e| e.security_software = Some(true),
                Blocked::SecuritySoftware,
            ),
            (|e| e.security_software = None, Blocked::SecurityUnknown),
            (|e| e.managed = Some(true), Blocked::Managed),
            (|e| e.managed = None, Blocked::ManagedUnknown),
            (|e| e.listed = 3, Blocked::Ambiguous),
            (|e| e.running = Some(true), Blocked::Running),
            (|e| e.running = None, Blocked::RunningUnknown),
        ];
        let package = id(PackageManager::HomebrewCask, "docker");
        let none = facts(Some(NONE));
        for (change, reason) in cases {
            let mut evidence = removable(&package, &none);
            change(&mut evidence);
            assert_eq!(homebrew_removal(&evidence), Err(vec![reason]), "{reason:?}");
        }
    }

    #[test]
    fn dependents_or_an_incomplete_count_refuse_and_protection_refuses() {
        let package = id(PackageManager::Homebrew, "openssl@3");
        for (needed_by, reason) in [
            (
                Some(RelationCount {
                    count: 2,
                    complete: true,
                }),
                Blocked::Dependents,
            ),
            (
                Some(RelationCount {
                    count: 1,
                    complete: false,
                }),
                Blocked::Dependents,
            ),
            (
                Some(RelationCount {
                    count: 0,
                    complete: false,
                }),
                Blocked::DependentsUnknown,
            ),
            (None, Blocked::DependentsUnknown),
        ] {
            let facts = facts(needed_by);
            assert_eq!(
                homebrew_removal(&removable(&package, &facts)),
                Err(vec![reason]),
                "{needed_by:?}"
            );
        }
        let mut protected = facts(Some(NONE));
        protected.protected = true;
        assert_eq!(
            homebrew_removal(&removable(&package, &protected)),
            Err(vec![Blocked::Protected])
        );
    }

    #[test]
    fn another_manager_is_never_run_as_homebrew() {
        let none = facts(Some(NONE));
        let package = id(PackageManager::Dpkg, "curl");
        assert_eq!(
            homebrew_removal(&removable(&package, &none)),
            Err(vec![Blocked::NotHomebrew])
        );
    }

    #[test]
    fn a_preview_binds_its_facts_and_tells_itself_apart() {
        let raw = |path: &str| {
            RawPath::from_unix_bytes(path.as_bytes().to_vec()).unwrap_or_else(|_| unreachable!())
        };
        let package = id(PackageManager::HomebrewCask, "hot");
        let folder = raw("/opt/homebrew/Caskroom/hot");
        let program = raw("/opt/homebrew/bin/brew");
        let identity = FileIdentity {
            volume: lumen_domain::VolumeId::new("dev:1").unwrap_or_else(|_| unreachable!()),
            file: lumen_domain::FileId::new(7),
        };
        let command = HomebrewCommand {
            args: vec!["uninstall".into(), "--cask".into(), "hot".into()],
        };
        let binding = Binding {
            key: "HomebrewCask:hot",
            package: &package,
            version: None,
            folder: &folder,
            identity: &identity,
            program: &program,
            command: &command,
            apps: &[],
        };
        let other = HomebrewCommand {
            args: vec!["uninstall".into(), "--formula".into(), "hot".into()],
        };
        assert_eq!(binding.hash(), binding.hash());
        assert_ne!(
            binding.hash(),
            Binding {
                command: &other,
                ..binding
            }
            .hash()
        );
        let decision = decision(binding.hash());
        assert_eq!(decision.policy_version(), GATE_VERSION);
        assert!(is_removal(std::slice::from_ref(&decision)));
        assert!(!is_removal(&[decision.clone(), decision.clone()]));
        assert!(any_removal(&[decision]));
        let app = crate::uninstall::decision(binding.hash());
        assert!(!any_removal(&[app]));
    }
}
