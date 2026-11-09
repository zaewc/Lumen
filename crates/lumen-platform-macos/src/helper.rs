//! The two ends of Lumen's helper connection on macOS (ADR-0063 rule 4).
//!
//! The helper is a launch daemon that listens on the Mach service
//! [`SERVICE`]. Each end requires the other to carry a Developer ID signature
//! by the same team as itself, with the other's code identifier:
//! [`APP_IDENTIFIER`] for Lumen, [`HELPER_IDENTIFIER`] for the helper. The team
//! is read from the running program's own signature, so a build that is not
//! Developer ID signed cannot use the helper at all, and no team is written
//! into the source.
//!
//! The app registers the daemon with `SMAppService` ([`register`]); macOS then
//! asks an administrator to allow it in System Settings › Login Items. Until
//! then [`registration`] says it requires approval, and launchd does not start
//! it (research 37).

use lumen_application::helper::{HelperRequest, HelperResponse, request, response};

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

/// Asks the helper for its status.
///
/// # Errors
///
/// [`HelperLinkError::Unsigned`] for a build without a Developer ID team, or
/// what XPC refused, including [`XpcError::BadReply`] for an answer that is not
/// one.
pub fn status() -> Result<HelperResponse, HelperLinkError> {
    let client = connect()?;
    let body = client.request(&request(&HelperRequest::Status))?;
    response(&body).ok_or(HelperLinkError::Xpc(XpcError::BadReply))
}

/// Where the daemon's registration stands (`SMAppService.status`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Registration {
    /// Not registered, or unregistered.
    NotRegistered,
    /// Registered and allowed: launchd starts it on demand.
    Enabled,
    /// Registered, but an administrator has not allowed it in System Settings
    /// › Login Items, or the person turned it off there.
    RequiresApproval,
    /// macOS found no such service: the plist is missing from the bundle.
    NotFound,
}

/// Why registering or unregistering failed: the `SMAppServiceErrorDomain` or
/// other `NSError` code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("macOS refused to change the helper's registration (error {code})")]
pub struct RegistrationError {
    /// The error's code.
    pub code: isize,
}

/// The daemon's registration now.
#[must_use]
pub fn registration() -> Registration {
    service::registration()
}

/// Registers the daemon. macOS adds it as waiting for an administrator's
/// approval in System Settings › Login Items; that is not an error.
///
/// # Errors
///
/// The `NSError` code when macOS refused and the daemon is neither enabled nor
/// waiting for approval afterwards.
pub fn register() -> Result<Registration, RegistrationError> {
    let result = service::register();
    match (registration(), result) {
        (now @ (Registration::Enabled | Registration::RequiresApproval), _) | (now, Ok(())) => {
            Ok(now)
        }
        (_, Err(code)) => Err(RegistrationError { code }),
    }
}

/// Unregisters the daemon; macOS stops it if it runs.
///
/// # Errors
///
/// The `NSError` code when macOS refused and it is still registered.
pub fn unregister() -> Result<(), RegistrationError> {
    let result = service::unregister();
    match (registration(), result) {
        (Registration::NotRegistered | Registration::NotFound, _) | (_, Ok(())) => Ok(()),
        (_, Err(code)) => Err(RegistrationError { code }),
    }
}

/// Opens System Settings at Login Items, where the person allows the helper.
pub fn open_login_items() {
    service::open_login_items();
}

/// `SMAppService` for the bundled daemon: the only `unsafe` code here.
#[allow(unsafe_code)]
mod service {
    use objc2::rc::Retained;
    use objc2_foundation::NSString;
    use objc2_service_management::{SMAppService, SMAppServiceStatus};

    use super::Registration;

    /// The plist in `Contents/Library/LaunchDaemons/`.
    const PLIST: &str = "dev.lumen.desktop.helper.plist";

    fn daemon() -> Retained<SMAppService> {
        // SAFETY: invariant: a service object for a plist name. Why unsafe:
        // Objective-C call. Why it holds: the name is a valid NSString; the
        // call only creates an object describing the service.
        unsafe { SMAppService::daemonServiceWithPlistName(&NSString::from_str(PLIST)) }
    }

    pub(super) fn registration() -> Registration {
        // SAFETY: invariant: reading a live service object's status. Why
        // unsafe: Objective-C call. Why it holds: `daemon()` returns a retained
        // object that lives for the call.
        let status = unsafe { daemon().status() };
        match status {
            SMAppServiceStatus::Enabled => Registration::Enabled,
            SMAppServiceStatus::RequiresApproval => Registration::RequiresApproval,
            SMAppServiceStatus::NotRegistered => Registration::NotRegistered,
            _ => Registration::NotFound,
        }
    }

    pub(super) fn register() -> Result<(), isize> {
        // SAFETY: invariant: registering a live service object. Why unsafe:
        // Objective-C call. Why it holds: the object lives for the call, and
        // the error, if any, is returned retained.
        unsafe { daemon().registerAndReturnError() }.map_err(|error| error.code())
    }

    pub(super) fn unregister() -> Result<(), isize> {
        // SAFETY: as for `register`.
        unsafe { daemon().unregisterAndReturnError() }.map_err(|error| error.code())
    }

    pub(super) fn open_login_items() {
        // SAFETY: invariant: none. Why unsafe: Objective-C class method. Why it
        // holds: it takes no arguments and only opens System Settings.
        unsafe { SMAppService::openSystemSettingsLoginItems() };
    }
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
