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
//!   rule 4, owner decision 2026-11-16).

use lumen_domain::{PackageId, PackageManager, RemovalFacts};

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
        let cases: [(Change, Blocked); 11] = [
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
}
