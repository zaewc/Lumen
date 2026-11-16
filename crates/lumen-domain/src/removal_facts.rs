//! What Lumen itself knows about removing a piece of installed software
//! (roadmap 16.6, #850): how its own mechanism removes it, whether that needs
//! administrator rights, and what its package records say it needs and what
//! needs it.
//!
//! These are observations from inventories and the records they read
//! (ADR-0060, ADR-0064), never the model's answer. The Apps page shows them
//! apart from AI review's note, each with its source (ADR-0061 rule 5), and
//! they decide over the note where the two disagree
//! ([`crate::SoftwareNote::guarded`]). They stay on the device: no field here
//! is sent to AI review (ADR-0061 rule 2, ADR-0064 rule 7).
//!
//! Unknown is never none (ADR-0008): a fact Lumen has no record of is `None`
//! or `false` as each field documents, and a count says whether every record
//! it rests on was read.

use crate::PackageManager;

/// How a piece of software's own mechanism removes it, as its record shows
/// (ADR-0060 rule 1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RemovalMechanism {
    /// Its package manager removes it: the software is a package of that
    /// manager, or a package of it recorded installing this application.
    PackageManager,
    /// Windows Installer removes it, by its product code.
    WindowsInstaller,
    /// Its installer recorded an uninstaller of its own.
    OwnUninstaller,
    /// Its record names no way to remove it: a macOS installer receipt, or a
    /// Windows entry with no removal command.
    NoneRecorded,
}

/// How many entries a relation reaches, and whether every record it rests on
/// was read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RelationCount {
    /// Entries the records name.
    pub count: u32,
    /// Every record it rests on was read, so `count` is all of them; otherwise
    /// it is at least `count`, and zero means unknown, never none.
    pub complete: bool,
}

impl RelationCount {
    /// Whether the records show at least one entry.
    pub const fn any(self) -> bool {
        self.count > 0
    }
}

/// What Lumen observed about removing one piece of software (#850).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct RemovalFacts {
    /// The platform marks it protected or part of the system: never removable
    /// from Lumen (ADR-0060).
    pub protected: bool,
    /// How its own mechanism removes it; `None` where no record says.
    pub mechanism: Option<RemovalMechanism>,
    /// For an application, the manager of the package whose record verifiably
    /// says it installed it: the source of `mechanism` and
    /// `needs_administrator`. `None` when they come from the software's own
    /// record, or nothing installed it verifiably.
    pub installed_by: Option<PackageManager>,
    /// Removing it is known to need administrator rights: its mechanism always
    /// does (a system package manager, a system-wide installer receipt).
    /// `false` where Lumen does not know, never "not needed".
    pub needs_administrator: bool,
    /// The packages its own record says it needs; `None` for software with no
    /// package record, or a record that was not read.
    pub needs: Option<RelationCount>,
    /// The listed packages whose own records say they need it, counted only
    /// where the record resolves to this one package (verified); `None` for
    /// software that is not a package.
    pub needed_by: Option<RelationCount>,
}

impl RemovalFacts {
    /// Whether Lumen's records show that other software relies on it.
    pub const fn relied_on(&self) -> bool {
        match self.needed_by {
            Some(count) => count.any(),
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_is_known_by_default() {
        let facts = RemovalFacts::default();
        assert!(!facts.protected);
        assert_eq!(facts.mechanism, None);
        assert!(!facts.needs_administrator);
        assert!(!facts.relied_on());
    }

    #[test]
    fn only_a_recorded_dependent_means_relied_on() {
        let with = |count, complete| RemovalFacts {
            needed_by: Some(RelationCount { count, complete }),
            ..RemovalFacts::default()
        };
        assert!(with(1, false).relied_on(), "at least one is enough");
        assert!(with(2, true).relied_on());
        assert!(!with(0, true).relied_on(), "none recorded");
        assert!(!with(0, false).relied_on(), "unknown is not relied on");
    }
}
