//! A small, safe layer over the C XPC API for Lumen's helper (ADR-0063, roadmap
//! 16.3b).
//!
//! A message carries one string, under [`BODY`]; what it means is the caller's
//! business (the helper speaks JSON). A [`Listener`] answers every request on
//! its own dispatch queue; a [`Client`] sends one request at a time and waits
//! for the answer.
//!
//! Both ends can require the peer to satisfy a code-signing requirement
//! (`xpc_connection_set_peer_code_signing_requirement`, macOS 12). XPC checks
//! it on every message: a listener drops requests from a peer that fails it,
//! and a client gets an error instead of the reply. [`developer_id`] builds the
//! requirement Lumen uses: a Developer ID signature by one team, for one code
//! identifier.

#![allow(unsafe_code)]

use std::ffi::{CStr, CString, c_char, c_int, c_void};
use std::ptr::NonNull;
use std::sync::Arc;

use block2::RcBlock;

/// The dictionary key that holds a message's string.
pub const BODY: &CStr = c"lumen";

/// The largest request or reply body accepted, in bytes.
pub const MAX_BODY: usize = 64 * 1024;

type Raw = *mut c_void;

/// `XPC_CONNECTION_MACH_SERVICE_LISTENER` (`xpc/connection.h`).
const MACH_SERVICE_LISTENER: u64 = 1 << 0;
/// `XPC_CONNECTION_MACH_SERVICE_PRIVILEGED` (`xpc/connection.h`): look the
/// service up in the system (launch daemon) domain.
const MACH_SERVICE_PRIVILEGED: u64 = 1 << 1;

// libxpc is part of libSystem, which every macOS program links.
unsafe extern "C" {
    fn xpc_connection_create(name: *const c_char, targetq: *mut c_void) -> Raw;
    fn xpc_connection_create_mach_service(
        name: *const c_char,
        targetq: *mut c_void,
        flags: u64,
    ) -> Raw;
    fn xpc_connection_create_from_endpoint(endpoint: Raw) -> Raw;
    fn xpc_endpoint_create(connection: Raw) -> Raw;
    fn xpc_connection_set_event_handler(connection: Raw, handler: &block2::Block<dyn Fn(Raw)>);
    fn xpc_connection_set_peer_code_signing_requirement(
        connection: Raw,
        requirement: *const c_char,
    ) -> c_int;
    fn xpc_connection_resume(connection: Raw);
    fn xpc_connection_cancel(connection: Raw);
    fn xpc_connection_send_message(connection: Raw, message: Raw);
    fn xpc_connection_send_message_with_reply_sync(connection: Raw, message: Raw) -> Raw;
    fn xpc_connection_get_euid(connection: Raw) -> libc::uid_t;
    fn xpc_dictionary_create(keys: *const *const c_char, values: *const Raw, count: usize) -> Raw;
    fn xpc_dictionary_create_reply(original: Raw) -> Raw;
    fn xpc_dictionary_set_string(dictionary: Raw, key: *const c_char, string: *const c_char);
    fn xpc_dictionary_get_string(dictionary: Raw, key: *const c_char) -> *const c_char;
    fn xpc_get_type(object: Raw) -> *const c_void;
    fn xpc_retain(object: Raw) -> Raw;
    fn xpc_release(object: Raw);
    static _xpc_type_connection: c_void;
    static _xpc_type_dictionary: c_void;
    static _xpc_type_error: c_void;
    static _xpc_error_connection_interrupted: c_void;
    static _xpc_error_connection_invalid: c_void;
    static _xpc_error_peer_code_signing_requirement: c_void;
}

/// What went wrong talking over XPC.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum XpcError {
    /// A service name or requirement held a NUL byte.
    #[error("a name or requirement holds a NUL byte")]
    Nul,
    /// XPC rejected the code-signing requirement string.
    #[error("XPC rejected the code-signing requirement")]
    BadRequirement,
    /// The peer does not satisfy the code-signing requirement.
    #[error("the other side is not signed as required")]
    NotSignedAsRequired,
    /// The service does not exist, or the connection was cancelled.
    #[error("the service is not available")]
    Unavailable,
    /// The other side went away before answering.
    #[error("the service went away before answering")]
    Interrupted,
    /// The answer had no body, or one too long or not UTF-8.
    #[error("the answer could not be read")]
    BadReply,
    /// The request is longer than [`MAX_BODY`].
    #[error("the request is too long")]
    TooLong,
}

/// An owned (+1) XPC object.
struct Object(NonNull<c_void>);

// SAFETY: invariant: XPC objects may be retained, released and sent from any
// thread. Why unsafe: a raw pointer is neither `Send` nor `Sync`. Why it holds:
// libxpc's reference counting is thread-safe, connections are documented as
// usable from any thread, and dictionaries here are only read after they are
// built.
unsafe impl Send for Object {}
// SAFETY: as for `Send` above.
unsafe impl Sync for Object {}

impl Object {
    /// Takes ownership of a +1 object, or `None` for null.
    fn adopt(raw: Raw) -> Option<Self> {
        NonNull::new(raw).map(Self)
    }

    /// Retains a borrowed object.
    fn retain(raw: Raw) -> Option<Self> {
        let raw = NonNull::new(raw)?;
        // SAFETY: invariant: `raw` is a live XPC object; `xpc_retain` adds a
        // reference and returns the same object. Why unsafe: FFI. Why it holds:
        // callers pass objects XPC handed them for the call's duration.
        unsafe { xpc_retain(raw.as_ptr()) };
        Some(Self(raw))
    }

    fn raw(&self) -> Raw {
        self.0.as_ptr()
    }

    /// A dictionary holding `body` under [`BODY`].
    fn message(body: &str) -> Result<Self, XpcError> {
        let body = CString::new(body).map_err(|_| XpcError::Nul)?;
        // SAFETY: invariant: an empty dictionary is created. Why unsafe: FFI.
        // Why it holds: null arrays with count 0 are the documented way.
        let message =
            Self::adopt(unsafe { xpc_dictionary_create(std::ptr::null(), std::ptr::null(), 0) })
                .ok_or(XpcError::Unavailable)?;
        // SAFETY: invariant: both strings are NUL-terminated and outlive the
        // call, which copies them. Why unsafe: FFI. Why it holds: `BODY` is a
        // static C string and `body` lives to the end of this function.
        unsafe { xpc_dictionary_set_string(message.raw(), BODY.as_ptr(), body.as_ptr()) };
        Ok(message)
    }
}

impl Drop for Object {
    fn drop(&mut self) {
        // SAFETY: invariant: `self` owns one reference. Why unsafe: FFI. Why it
        // holds: every `Object` was adopted or retained exactly once.
        unsafe { xpc_release(self.raw()) };
    }
}

/// The kind of an XPC object.
#[derive(PartialEq, Eq)]
enum Kind {
    Connection,
    Dictionary,
    Error,
    Other,
}

fn kind(object: Raw) -> Kind {
    // SAFETY: invariant: `object` is a live XPC object and the type symbols are
    // immutable statics of libxpc. Why unsafe: FFI and extern statics. Why it
    // holds: XPC hands the object to the caller; only addresses are compared.
    unsafe {
        let kind = xpc_get_type(object);
        if std::ptr::eq(kind, &raw const _xpc_type_connection) {
            Kind::Connection
        } else if std::ptr::eq(kind, &raw const _xpc_type_dictionary) {
            Kind::Dictionary
        } else if std::ptr::eq(kind, &raw const _xpc_type_error) {
            Kind::Error
        } else {
            Kind::Other
        }
    }
}

/// What an XPC error object means.
fn error_of(object: Raw) -> XpcError {
    // Only the addresses of libxpc's global error objects are compared.
    let object = object.cast_const();
    if std::ptr::eq(object, &raw const _xpc_error_peer_code_signing_requirement) {
        XpcError::NotSignedAsRequired
    } else if std::ptr::eq(object, &raw const _xpc_error_connection_interrupted) {
        XpcError::Interrupted
    } else if std::ptr::eq(object, &raw const _xpc_error_connection_invalid) {
        XpcError::Unavailable
    } else {
        XpcError::BadReply
    }
}

/// The body of a dictionary message, if it has one of at most [`MAX_BODY`]
/// bytes that is UTF-8.
fn body_of(message: Raw) -> Option<String> {
    // SAFETY: invariant: `message` is a live dictionary; the returned pointer
    // is valid while it lives. Why unsafe: FFI and a C string read. Why it
    // holds: the string is copied before `message` can be released.
    let text = unsafe {
        let raw = xpc_dictionary_get_string(message, BODY.as_ptr());
        if raw.is_null() {
            return None;
        }
        CStr::from_ptr(raw)
    };
    let bytes = text.to_bytes();
    (bytes.len() <= MAX_BODY)
        .then(|| String::from_utf8(bytes.to_vec()).ok())
        .flatten()
}

/// Sets `requirement` on a connection that is not yet resumed. On failure the
/// connection is cancelled and resumed, since releasing a connection that was
/// never resumed is a programming error XPC traps on.
fn require(connection: &Object, requirement: Option<&str>) -> Result<(), XpcError> {
    let result = set_requirement(connection, requirement);
    if result.is_err() {
        let ignore = RcBlock::new(|_: Raw| {});
        // SAFETY: invariant: a suspended connection gets a handler, is
        // cancelled, then resumed so the cancellation is delivered. Why unsafe:
        // FFI. Why it holds: the connection was never resumed before.
        unsafe {
            xpc_connection_set_event_handler(connection.raw(), &ignore);
            xpc_connection_cancel(connection.raw());
            xpc_connection_resume(connection.raw());
        }
    }
    result
}

fn set_requirement(connection: &Object, requirement: Option<&str>) -> Result<(), XpcError> {
    let Some(requirement) = requirement else {
        return Ok(());
    };
    let requirement = CString::new(requirement).map_err(|_| XpcError::Nul)?;
    // SAFETY: invariant: called once, before the connection is resumed, with a
    // NUL-terminated string the call copies. Why unsafe: FFI. Why it holds:
    // only `listen` and `connect` call it, once each, before resuming.
    let status = unsafe {
        xpc_connection_set_peer_code_signing_requirement(connection.raw(), requirement.as_ptr())
    };
    if status == 0 {
        Ok(())
    } else {
        Err(XpcError::BadRequirement)
    }
}

/// Who sent a request, from the connection rather than the request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Peer {
    /// The effective user ID of the sending process.
    pub euid: u32,
}

/// Answers a request body from a peer with a reply body.
pub type Handler = Arc<dyn Fn(Peer, &str) -> String + Send + Sync>;

/// A listening connection; dropping it cancels it.
pub struct Listener {
    connection: Object,
}

impl std::fmt::Debug for Listener {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Listener").finish_non_exhaustive()
    }
}

impl Listener {
    /// Listens on the launchd Mach service `name`, answering each request with
    /// `handler`. With `requirement`, requests from a peer that does not
    /// satisfy it are dropped unanswered.
    ///
    /// # Errors
    ///
    /// [`XpcError::Nul`] or [`XpcError::BadRequirement`] for a malformed name
    /// or requirement, or [`XpcError::Unavailable`] if XPC made no connection.
    pub fn mach_service(
        name: &str,
        requirement: Option<&str>,
        handler: Handler,
    ) -> Result<Self, XpcError> {
        let name = CString::new(name).map_err(|_| XpcError::Nul)?;
        // SAFETY: invariant: a listener for a NUL-terminated name, on XPC's
        // default queue. Why unsafe: FFI. Why it holds: the name is copied.
        let connection = Object::adopt(unsafe {
            xpc_connection_create_mach_service(
                name.as_ptr(),
                std::ptr::null_mut(),
                MACH_SERVICE_LISTENER,
            )
        })
        .ok_or(XpcError::Unavailable)?;
        Self::start(connection, requirement, handler)
    }

    /// An anonymous listener, reachable only through its [`Listener::endpoint`]:
    /// for tests in one process.
    ///
    /// # Errors
    ///
    /// As [`Listener::mach_service`].
    pub fn anonymous(requirement: Option<&str>, handler: Handler) -> Result<Self, XpcError> {
        // SAFETY: invariant: a null name makes an anonymous listener. Why
        // unsafe: FFI. Why it holds: documented in `xpc/connection.h`.
        let connection =
            Object::adopt(unsafe { xpc_connection_create(std::ptr::null(), std::ptr::null_mut()) })
                .ok_or(XpcError::Unavailable)?;
        Self::start(connection, requirement, handler)
    }

    fn start(
        connection: Object,
        requirement: Option<&str>,
        handler: Handler,
    ) -> Result<Self, XpcError> {
        require(&connection, requirement)?;
        // Set again on each accepted peer: the header does not say whether a
        // peer inherits the listener's requirement (the newer listener API
        // says peers do not).
        let requirement = requirement.map(str::to_owned);
        let accept = RcBlock::new(move |event: Raw| {
            if kind(event) == Kind::Connection {
                serve(event, requirement.as_deref(), Arc::clone(&handler));
            }
        });
        // SAFETY: invariant: the handler is set before the listener is resumed;
        // XPC copies the block. Why unsafe: FFI. Why it holds: done once, here.
        unsafe {
            xpc_connection_set_event_handler(connection.raw(), &accept);
            xpc_connection_resume(connection.raw());
        }
        Ok(Self { connection })
    }

    /// The listener's endpoint, for [`Client::endpoint`].
    #[must_use]
    pub fn endpoint(&self) -> Endpoint {
        // SAFETY: invariant: an endpoint for a live listener. Why unsafe: FFI.
        // Why it holds: `self.connection` lives for the call.
        Endpoint(Object::adopt(unsafe {
            xpc_endpoint_create(self.connection.raw())
        }))
    }
}

impl Drop for Listener {
    fn drop(&mut self) {
        // SAFETY: invariant: cancelling a resumed connection once. Why unsafe:
        // FFI. Why it holds: only this drop cancels it.
        unsafe { xpc_connection_cancel(self.connection.raw()) };
    }
}

/// Accepts one peer connection and answers its requests.
fn serve(peer: Raw, requirement: Option<&str>, handler: Handler) {
    let Some(peer) = Object::retain(peer) else {
        return;
    };
    // A peer whose requirement cannot be set is cancelled unanswered.
    if require(&peer, requirement).is_err() {
        return;
    }
    let connection = Arc::new(peer);
    let reply_to = Arc::clone(&connection);
    let messages = RcBlock::new(move |message: Raw| {
        if kind(message) != Kind::Dictionary {
            return;
        }
        // SAFETY: invariant: reading the effective uid XPC recorded for the
        // live peer connection. Why unsafe: FFI. Why it holds: `reply_to` keeps
        // the connection alive.
        let euid = unsafe { xpc_connection_get_euid(reply_to.raw()) };
        // A request without a readable body, or an answer that cannot be sent
        // as a C string, still gets a reply (without a body), so the sender
        // never waits for one that will not come.
        let answer = body_of(message)
            .map(|body| handler(Peer { euid }, &body))
            .and_then(|answer| CString::new(answer).ok());
        // SAFETY: invariant: a reply to a live request message, or null when no
        // reply is expected. Why unsafe: FFI. Why it holds: `message` is the
        // event XPC passed in.
        let Some(reply) = Object::adopt(unsafe { xpc_dictionary_create_reply(message) }) else {
            return;
        };
        // SAFETY: invariant: the strings are NUL-terminated and copied; the
        // reply goes back on the connection it came from. Why unsafe: FFI. Why
        // it holds: all three objects are alive here.
        unsafe {
            if let Some(answer) = &answer {
                xpc_dictionary_set_string(reply.raw(), BODY.as_ptr(), answer.as_ptr());
            }
            xpc_connection_send_message(reply_to.raw(), reply.raw());
        }
    });
    // SAFETY: invariant: the peer's handler is set before it is resumed. Why
    // unsafe: FFI. Why it holds: done once for each accepted peer.
    unsafe {
        xpc_connection_set_event_handler(connection.raw(), &messages);
        xpc_connection_resume(connection.raw());
    }
}

/// A listener's address within one process, for tests.
pub struct Endpoint(Option<Object>);

impl std::fmt::Debug for Endpoint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Endpoint").finish_non_exhaustive()
    }
}

/// A connection to a listener; dropping it cancels it.
pub struct Client {
    connection: Object,
}

impl std::fmt::Debug for Client {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Client").finish_non_exhaustive()
    }
}

impl Client {
    /// Connects to the launch daemon's Mach service `name`, in the system
    /// domain. With `requirement`, replies from a service that does not
    /// satisfy it are refused.
    ///
    /// # Errors
    ///
    /// [`XpcError::Nul`] or [`XpcError::BadRequirement`] for a malformed name
    /// or requirement, or [`XpcError::Unavailable`].
    pub fn mach_service(name: &str, requirement: Option<&str>) -> Result<Self, XpcError> {
        let name = CString::new(name).map_err(|_| XpcError::Nul)?;
        // SAFETY: invariant: a client connection for a NUL-terminated name.
        // Why unsafe: FFI. Why it holds: the name is copied.
        let connection = Object::adopt(unsafe {
            xpc_connection_create_mach_service(
                name.as_ptr(),
                std::ptr::null_mut(),
                MACH_SERVICE_PRIVILEGED,
            )
        })
        .ok_or(XpcError::Unavailable)?;
        Self::start(connection, requirement)
    }

    /// Connects to a listener in this process.
    ///
    /// # Errors
    ///
    /// As [`Client::mach_service`].
    pub fn endpoint(endpoint: &Endpoint, requirement: Option<&str>) -> Result<Self, XpcError> {
        let endpoint = endpoint.0.as_ref().ok_or(XpcError::Unavailable)?;
        // SAFETY: invariant: a connection from a live endpoint. Why unsafe:
        // FFI. Why it holds: `endpoint` lives for the call.
        let connection =
            Object::adopt(unsafe { xpc_connection_create_from_endpoint(endpoint.raw()) })
                .ok_or(XpcError::Unavailable)?;
        Self::start(connection, requirement)
    }

    fn start(connection: Object, requirement: Option<&str>) -> Result<Self, XpcError> {
        require(&connection, requirement)?;
        // Errors arrive as replies to `request`; nothing else is expected.
        let ignore = RcBlock::new(|_: Raw| {});
        // SAFETY: invariant: a handler is set before resuming, as XPC requires.
        // Why unsafe: FFI. Why it holds: done once, here.
        unsafe {
            xpc_connection_set_event_handler(connection.raw(), &ignore);
            xpc_connection_resume(connection.raw());
        }
        Ok(Self { connection })
    }

    /// Sends `body` and waits for the reply's body.
    ///
    /// # Errors
    ///
    /// [`XpcError::TooLong`] or [`XpcError::Nul`] for a body that cannot be
    /// sent; otherwise what XPC answered instead of a reply.
    pub fn request(&self, body: &str) -> Result<String, XpcError> {
        if body.len() > MAX_BODY {
            return Err(XpcError::TooLong);
        }
        let message = Object::message(body)?;
        // SAFETY: invariant: a synchronous request on a resumed connection,
        // returning a +1 reply or error. Why unsafe: FFI. Why it holds: never
        // called on XPC's own queue, which would deadlock: `Client` is used
        // from ordinary threads only.
        let reply = Object::adopt(unsafe {
            xpc_connection_send_message_with_reply_sync(self.connection.raw(), message.raw())
        })
        .ok_or(XpcError::BadReply)?;
        match kind(reply.raw()) {
            Kind::Dictionary => body_of(reply.raw()).ok_or(XpcError::BadReply),
            Kind::Error => Err(error_of(reply.raw())),
            Kind::Connection | Kind::Other => Err(XpcError::BadReply),
        }
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        // SAFETY: invariant: cancelling a resumed connection once. Why unsafe:
        // FFI. Why it holds: only this drop cancels it.
        unsafe { xpc_connection_cancel(self.connection.raw()) };
    }
}

/// The code-signing requirement for code signed with a Developer ID by `team`
/// under the code identifier `identifier` (TN3127): Apple's anchor, the
/// Developer ID intermediate and leaf markers, the team and the identifier.
///
/// `None` if `team` or `identifier` holds anything but ASCII letters, digits,
/// `.` and `-`, so neither can change the requirement's meaning.
#[must_use]
pub fn developer_id(team: &str, identifier: &str) -> Option<String> {
    let plain = |s: &str| {
        !s.is_empty()
            && s.bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-')
    };
    (plain(team) && plain(identifier)).then(|| {
        format!(
            "anchor apple generic and certificate 1[field.1.2.840.113635.100.6.2.6] exists \
             and certificate leaf[field.1.2.840.113635.100.6.1.13] exists \
             and certificate leaf[subject.OU] = \"{team}\" and identifier \"{identifier}\""
        )
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use std::sync::mpsc;
    use std::time::Duration;

    use super::*;

    fn echo() -> Handler {
        Arc::new(|peer: Peer, body: &str| format!("{}:{body}", peer.euid))
    }

    /// Runs `f` on its own thread and gives up after a few seconds, so a
    /// dropped request cannot hang the test run.
    fn within<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> Option<T> {
        let (send, receive) = mpsc::channel();
        std::thread::spawn(move || {
            // The receiver is gone only after the timeout, when nobody waits.
            send.send(f()).ok();
        });
        receive.recv_timeout(Duration::from_secs(10)).ok()
    }

    #[test]
    fn a_request_is_answered_with_the_sender_s_uid() {
        let listener = Listener::anonymous(None, echo()).unwrap();
        let client = Client::endpoint(&listener.endpoint(), None).unwrap();
        // SAFETY: invariant: none. Why unsafe: FFI. Why it holds: no arguments.
        let uid = unsafe { libc::geteuid() };
        assert_eq!(client.request("status").unwrap(), format!("{uid}:status"));
        assert_eq!(client.request("again").unwrap(), format!("{uid}:again"));
    }

    #[test]
    fn a_peer_that_satisfies_the_listener_s_requirement_is_answered() {
        // The test binary is ad hoc signed, so it is not anchored at Apple;
        // the requirement is set on the listener and on the accepted peer.
        let listener = Listener::anonymous(Some("! anchor apple"), echo()).unwrap();
        let client = Client::endpoint(&listener.endpoint(), None).unwrap();
        let answer = within(move || client.request("status"));
        assert!(matches!(answer, Some(Ok(ref body)) if body.ends_with(":status")));
    }

    #[test]
    fn a_listener_ignores_a_peer_that_is_not_signed_as_required() {
        // The test binary is not signed by Apple, so it fails "anchor apple".
        let listener = Listener::anonymous(Some("anchor apple"), echo()).unwrap();
        let endpoint = listener.endpoint();
        let client = Client::endpoint(&endpoint, None).unwrap();
        let answer = within(move || client.request("status"));
        assert!(
            !matches!(answer, Some(Ok(_))),
            "an unsigned peer got an answer: {answer:?}"
        );
        drop(listener);
    }

    #[test]
    fn a_client_refuses_a_listener_that_is_not_signed_as_required() {
        let listener = Listener::anonymous(None, echo()).unwrap();
        let client = Client::endpoint(&listener.endpoint(), Some("anchor apple")).unwrap();
        let answer = within(move || client.request("status"));
        assert_eq!(answer, Some(Err(XpcError::NotSignedAsRequired)));
    }

    #[test]
    fn a_malformed_requirement_or_long_body_is_refused() {
        assert_eq!(
            Listener::anonymous(Some("not a requirement ((("), echo()).err(),
            Some(XpcError::BadRequirement)
        );
        let listener = Listener::anonymous(None, echo()).unwrap();
        let client = Client::endpoint(&listener.endpoint(), None).unwrap();
        assert_eq!(
            client.request(&"x".repeat(MAX_BODY + 1)),
            Err(XpcError::TooLong)
        );
        assert_eq!(client.request("a\0b"), Err(XpcError::Nul));
    }

    #[test]
    fn a_missing_service_is_unavailable() {
        let client = Client::mach_service("dev.lumen.test.no-such-service", None).unwrap();
        assert_eq!(client.request("status"), Err(XpcError::Unavailable));
    }

    #[test]
    fn the_developer_id_requirement_names_the_team_and_identifier() {
        let requirement = developer_id("UB2797YAAH", "dev.lumen.desktop").unwrap();
        assert!(requirement.contains("leaf[subject.OU] = \"UB2797YAAH\""));
        assert!(requirement.ends_with("identifier \"dev.lumen.desktop\""));
        assert_eq!(developer_id("TEAM\" or true", "dev.lumen.desktop"), None);
        assert_eq!(developer_id("", "dev.lumen.desktop"), None);
        // XPC accepts it as a requirement.
        let listener = Listener::anonymous(Some(&requirement), echo());
        assert!(listener.is_ok());
    }
}
