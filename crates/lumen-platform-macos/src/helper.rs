//! The two ends of Lumen's helper connection on macOS (ADR-0063 rule 4).
//!
//! The helper is a launch daemon that listens on the Mach service
//! [`SERVICE`]. Each end requires the other to carry a Developer ID signature
//! by the same team as itself, with the other's code identifier:
//! [`APP_IDENTIFIER`] for Lumen, [`HELPER_IDENTIFIER`] for the helper. The team
//! is read from the running program's own signature, so a build that is not
//! Developer ID signed cannot use the helper at all, and no team is written
//! into the source.

use crate::xpc::{Client, Handler, Listener, XpcError, developer_id};

/// The helper's launchd label and Mach service name.
pub const SERVICE: &str = "dev.lumen.desktop.helper";

/// Lumen's code identifier (its bundle identifier).
pub const APP_IDENTIFIER: &str = "dev.lumen.desktop";

/// The helper's code identifier, embedded in its Info.plist section.
///
/// Never change it: launchd ties a registered daemon to its signature's
/// identifier, and a daemon whose identifier changed in an update stops
/// launching (Apple DTS, research 37).
pub const HELPER_IDENTIFIER: &str = "dev.lumen.desktop.helper";

/// Why the helper connection could not be set up.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum HelperLinkError {
    /// This program carries no valid signature with a team, so the other end
    /// could never be checked: only Developer ID signed builds use the helper.
    #[error("this build is not signed with a Developer ID, so it cannot use Lumen's helper")]
    Unsigned,
    /// XPC refused.
    #[error(transparent)]
    Xpc(#[from] XpcError),
}

/// The Team ID of this program's own valid signature, if it has one.
#[must_use]
pub fn own_team() -> Option<String> {
    let exe = std::env::current_exe().ok()?;
    let url = objc2_core_foundation::CFURL::from_file_path(&exe)?;
    let signature = crate::apps::verify_signature(&url);
    if signature.check != lumen_domain::SignatureCheck::SignerVerified {
        return None;
    }
    signature.signer.map(|team| team.raw().to_owned())
}

/// The requirement for the other end, signed by this program's team.
fn requirement(identifier: &str) -> Result<String, HelperLinkError> {
    own_team()
        .and_then(|team| developer_id(&team, identifier))
        .ok_or(HelperLinkError::Unsigned)
}

/// Starts listening as the helper, answering only Lumen of the same team.
///
/// # Errors
///
/// [`HelperLinkError::Unsigned`] for a build without a Developer ID team, or
/// what XPC refused.
pub fn serve(handler: Handler) -> Result<Listener, HelperLinkError> {
    let requirement = requirement(APP_IDENTIFIER)?;
    Ok(Listener::mach_service(
        SERVICE,
        Some(&requirement),
        handler,
    )?)
}

/// Connects to the helper, accepting only the helper of the same team.
///
/// # Errors
///
/// [`HelperLinkError::Unsigned`] for a build without a Developer ID team, or
/// what XPC refused.
pub fn connect() -> Result<Client, HelperLinkError> {
    let requirement = requirement(HELPER_IDENTIFIER)?;
    Ok(Client::mach_service(SERVICE, Some(&requirement))?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unsigned_test_build_cannot_use_the_helper() {
        // Test binaries carry at most an ad-hoc signature, with no team.
        assert_eq!(own_team(), None);
        assert_eq!(connect().err(), Some(HelperLinkError::Unsigned));
        let handler: Handler = std::sync::Arc::new(|_, _: &str| String::new());
        assert_eq!(serve(handler).err(), Some(HelperLinkError::Unsigned));
    }

    #[test]
    fn the_identifiers_make_valid_requirements() {
        for identifier in [APP_IDENTIFIER, HELPER_IDENTIFIER] {
            assert!(developer_id("UB2797YAAH", identifier).is_some());
        }
    }
}
