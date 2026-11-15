//! The folders a scan could not read in full (ADR-0008), for people to see.
//! Paths are display strings, never handed back to act on.

use lumen_app::unread::{Unread, UnreadFolder, UnreadFolders};
use lumen_domain::{DenialReason, FailureKind};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Why a folder was not read in full.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum UnreadReasonView {
    /// macOS privacy protection: Full Disk Access may help.
    Privacy,
    /// macOS System Integrity Protection; nothing to change.
    SystemProtected,
    /// File permissions or an access-control list refused it.
    Permissions,
    /// Outside what the platform lets Lumen read.
    OutOfScope,
    /// Refused for a reason the platform did not say.
    Denied,
    /// It disappeared while being scanned.
    Vanished,
    /// Reading it took too long.
    TimedOut,
    /// Reading it failed.
    Failed,
    /// Read, but not to the end.
    CutShort,
    /// Never entered: another disk, an excluded folder, or not reached.
    NotEntered,
}

impl From<Unread> for UnreadReasonView {
    fn from(why: Unread) -> Self {
        match why {
            Unread::Denied(DenialReason::Tcc) => Self::Privacy,
            Unread::Denied(DenialReason::Sip) => Self::SystemProtected,
            Unread::Denied(DenialReason::Posix | DenialReason::Acl) => Self::Permissions,
            Unread::Denied(DenialReason::Scope) => Self::OutOfScope,
            Unread::Failed(FailureKind::Vanished) => Self::Vanished,
            Unread::Failed(FailureKind::TimedOut) => Self::TimedOut,
            Unread::CutShort => Self::CutShort,
            Unread::NotEntered => Self::NotEntered,
            // A reason or kind added later: say only that much.
            Unread::Denied(_) => Self::Denied,
            _ => Self::Failed,
        }
    }
}

/// One folder not read in full.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct UnreadFolderView {
    /// Where it is.
    pub path: String,
    /// Why it was not read.
    pub reason: UnreadReasonView,
}

impl From<&UnreadFolder> for UnreadFolderView {
    fn from(folder: &UnreadFolder) -> Self {
        Self {
            path: folder.path.display(),
            reason: folder.why.into(),
        }
    }
}

/// The folders a scan did not read in full.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub struct UnreadFoldersView {
    /// The scan's ID.
    pub scan: String,
    /// How many there are, as a decimal string; may exceed `folders`.
    pub total: String,
    /// The first of them, in path order.
    pub folders: Vec<UnreadFolderView>,
}

impl From<&UnreadFolders> for UnreadFoldersView {
    fn from(unread: &UnreadFolders) -> Self {
        Self {
            scan: unread.scan.to_string(),
            total: unread.total.to_string(),
            folders: unread.folders.iter().map(UnreadFolderView::from).collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reasons_say_what_a_person_can_do() {
        let cases = [
            (Unread::Denied(DenialReason::Tcc), "privacy"),
            (Unread::Denied(DenialReason::Sip), "system_protected"),
            (Unread::Denied(DenialReason::Acl), "permissions"),
            (Unread::Denied(DenialReason::Posix), "permissions"),
            (Unread::Denied(DenialReason::Scope), "out_of_scope"),
            (Unread::Denied(DenialReason::Unknown), "denied"),
            (Unread::Failed(FailureKind::Vanished), "vanished"),
            (Unread::Failed(FailureKind::TimedOut), "timed_out"),
            (Unread::Failed(FailureKind::Io), "failed"),
            (Unread::CutShort, "cut_short"),
            (Unread::NotEntered, "not_entered"),
        ];
        for (why, code) in cases {
            assert_eq!(
                serde_json::to_value(UnreadReasonView::from(why)).ok(),
                Some(serde_json::Value::from(code)),
                "{why:?}"
            );
        }
    }
}
