//! Lumen's privileged helper as the app sees it (ADR-0063, roadmap 16.3b).
//!
//! The helper is opt-in. [`enable`] registers it as a launch daemon, and
//! macOS then asks an administrator to allow it in System Settings › Login
//! Items; [`disable`] unregisters it. [`state`] says where the registration
//! stands and, once it is allowed, asks the helper for its status over XPC,
//! which also proves the two ends accept each other's signatures.
//!
//! Only Developer ID signed builds can use the helper; elsewhere, and off
//! macOS, these calls say so. Nothing here moves or reads any file.

/// Where the helper's registration stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HelperRegistration {
    /// This system has no helper (only macOS, so far).
    Unsupported,
    /// It is not set up.
    NotRegistered,
    /// It is set up and allowed.
    Enabled,
    /// It is set up, but an administrator has not allowed it in System
    /// Settings › Login Items, or it was turned off there.
    RequiresApproval,
    /// macOS cannot find it in this copy of Lumen.
    NotFound,
}

/// What asking the helper for its status gave.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HelperReach {
    /// It was not asked, because it is not enabled.
    NotAsked,
    /// It answered.
    Answered {
        /// Its protocol version.
        protocol: u32,
        /// Its version.
        version: String,
        /// Its protocol and version are this app's; otherwise set it up again.
        current: bool,
    },
    /// This build is not signed with a Developer ID, so it cannot use the
    /// helper.
    Unsigned,
    /// The helper did not answer: not running, or not started by launchd.
    Unavailable,
    /// The helper is not signed by the same team as this app.
    NotSignedAsRequired,
    /// The helper refused the request.
    Refused,
    /// The answer could not be read.
    Failed,
}

/// The helper as it stands now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HelperState {
    /// Its registration.
    pub registration: HelperRegistration,
    /// Its answer, when enabled.
    pub reach: HelperReach,
}

/// Why the helper could not be set up or removed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum HelperError {
    /// This system has no helper.
    #[error("Lumen's helper is available only on macOS")]
    Unsupported,
    /// This build is not signed with a Developer ID.
    #[error("this build is not signed with a Developer ID, so it cannot use Lumen's helper")]
    Unsigned,
    /// Lumen runs with administrator rights itself, which it never does.
    #[error("Lumen is running with administrator rights; start it normally")]
    Privileged,
    /// macOS refused, with this error code.
    #[error("macOS refused to change the helper's registration (error {0})")]
    Refused(isize),
}

/// The helper's state now; asks it for its status when it is enabled.
#[must_use]
pub fn state() -> HelperState {
    #[cfg(target_os = "macos")]
    {
        mac::state()
    }
    #[cfg(not(target_os = "macos"))]
    {
        HelperState {
            registration: HelperRegistration::Unsupported,
            reach: HelperReach::NotAsked,
        }
    }
}

/// Sets the helper up. macOS then asks an administrator to allow it in System
/// Settings › Login Items.
///
/// # Errors
///
/// A [`HelperError`]: off macOS, an unsigned or privileged Lumen, or macOS
/// refusing.
pub fn enable() -> Result<HelperState, HelperError> {
    #[cfg(target_os = "macos")]
    {
        mac::enable()
    }
    #[cfg(not(target_os = "macos"))]
    {
        Err(HelperError::Unsupported)
    }
}

/// Removes the helper; macOS stops it if it runs.
///
/// # Errors
///
/// A [`HelperError`]: off macOS, or macOS refusing.
pub fn disable() -> Result<HelperState, HelperError> {
    #[cfg(target_os = "macos")]
    {
        mac::disable()
    }
    #[cfg(not(target_os = "macos"))]
    {
        Err(HelperError::Unsupported)
    }
}

/// Opens System Settings at Login Items, where the helper is allowed.
///
/// # Errors
///
/// [`HelperError::Unsupported`] off macOS.
pub fn open_login_items() -> Result<(), HelperError> {
    #[cfg(target_os = "macos")]
    {
        lumen_platform_macos::helper::open_login_items();
        Ok(())
    }
    #[cfg(not(target_os = "macos"))]
    {
        Err(HelperError::Unsupported)
    }
}

/// What a status answer means for this app, whose protocol is `protocol` and
/// version `version`.
#[cfg_attr(
    not(target_os = "macos"),
    allow(dead_code, reason = "only macOS asks the helper outside tests")
)]
fn reach_of(
    answer: Result<lumen_application::helper::HelperResponse, Link>,
    version: &str,
) -> HelperReach {
    use lumen_application::helper::{HelperResponse, PROTOCOL};
    match answer {
        Ok(HelperResponse::Status {
            protocol,
            version: theirs,
            ..
        }) => HelperReach::Answered {
            current: protocol == PROTOCOL && theirs == version,
            protocol,
            version: theirs,
        },
        Ok(HelperResponse::Refused { .. }) => HelperReach::Refused,
        Err(Link::Unsigned) => HelperReach::Unsigned,
        Err(Link::Unavailable) => HelperReach::Unavailable,
        Err(Link::NotSignedAsRequired) => HelperReach::NotSignedAsRequired,
        Ok(_) | Err(Link::Failed) => HelperReach::Failed,
    }
}

/// Why the helper could not be asked, independent of the platform.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(
    not(target_os = "macos"),
    allow(dead_code, reason = "only macOS asks the helper outside tests")
)]
enum Link {
    Unsigned,
    Unavailable,
    NotSignedAsRequired,
    Failed,
}

#[cfg(target_os = "macos")]
mod mac {
    //! This Mac's `SMAppService` registration and the XPC status call.

    use lumen_platform_macos::helper::{self as platform, HelperLinkError, Registration};
    use lumen_platform_macos::xpc::XpcError;

    use super::{HelperError, HelperReach, HelperRegistration, HelperState, Link, reach_of};

    fn registration() -> HelperRegistration {
        match platform::registration() {
            Registration::NotRegistered => HelperRegistration::NotRegistered,
            Registration::Enabled => HelperRegistration::Enabled,
            Registration::RequiresApproval => HelperRegistration::RequiresApproval,
            Registration::NotFound => HelperRegistration::NotFound,
        }
    }

    fn link(error: HelperLinkError) -> Link {
        match error {
            HelperLinkError::Unsigned => Link::Unsigned,
            HelperLinkError::Xpc(XpcError::NotSignedAsRequired) => Link::NotSignedAsRequired,
            HelperLinkError::Xpc(XpcError::Unavailable | XpcError::Interrupted) => {
                Link::Unavailable
            }
            _ => Link::Failed,
        }
    }

    pub(super) fn state() -> HelperState {
        let registration = registration();
        let reach = if registration == HelperRegistration::Enabled {
            reach_of(platform::status().map_err(link), env!("CARGO_PKG_VERSION"))
        } else {
            HelperReach::NotAsked
        };
        HelperState {
            registration,
            reach,
        }
    }

    pub(super) fn enable() -> Result<HelperState, HelperError> {
        crate::store::refuse_privileged().map_err(|_| HelperError::Privileged)?;
        if platform::own_team().is_none() {
            return Err(HelperError::Unsigned);
        }
        platform::register().map_err(|error| HelperError::Refused(error.code))?;
        tracing::info!("helper registered");
        Ok(state())
    }

    pub(super) fn disable() -> Result<HelperState, HelperError> {
        platform::unregister().map_err(|error| HelperError::Refused(error.code))?;
        tracing::info!("helper unregistered");
        Ok(state())
    }
}

#[cfg(test)]
mod tests {
    use lumen_application::helper::{HelperRefusal, HelperResponse, PROTOCOL};

    use super::*;

    fn status(protocol: u32, version: &str) -> HelperResponse {
        HelperResponse::Status {
            protocol,
            version: version.into(),
            caller_uid: 501,
        }
    }

    #[test]
    fn a_helper_of_this_version_is_current() {
        assert_eq!(
            reach_of(Ok(status(PROTOCOL, "0.11.0")), "0.11.0"),
            HelperReach::Answered {
                protocol: PROTOCOL,
                version: "0.11.0".into(),
                current: true,
            }
        );
    }

    #[test]
    fn a_helper_of_another_version_or_protocol_is_not_current() {
        for answer in [status(PROTOCOL, "0.10.0"), status(PROTOCOL + 1, "0.11.0")] {
            assert!(matches!(
                reach_of(Ok(answer), "0.11.0"),
                HelperReach::Answered { current: false, .. }
            ));
        }
    }

    #[test]
    fn failures_say_what_went_wrong() {
        let refused = Ok(HelperResponse::Refused {
            reason: HelperRefusal::RootCaller,
        });
        assert_eq!(reach_of(refused, "1"), HelperReach::Refused);
        assert_eq!(reach_of(Err(Link::Unsigned), "1"), HelperReach::Unsigned);
        assert_eq!(
            reach_of(Err(Link::Unavailable), "1"),
            HelperReach::Unavailable
        );
        assert_eq!(
            reach_of(Err(Link::NotSignedAsRequired), "1"),
            HelperReach::NotSignedAsRequired
        );
        assert_eq!(reach_of(Err(Link::Failed), "1"), HelperReach::Failed);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn an_unsigned_test_build_cannot_set_the_helper_up() {
        // Test binaries carry no Developer ID, so nothing is registered.
        assert!(matches!(
            enable(),
            Err(HelperError::Unsigned | HelperError::Privileged)
        ));
    }
}
