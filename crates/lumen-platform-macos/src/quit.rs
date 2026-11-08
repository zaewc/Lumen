//! Asking a running application to quit (roadmap 16.3a).
//!
//! [`ask_to_quit`] sends each instance of one application bundle the same
//! request as Quit in its menu (`-[NSRunningApplication terminate]`), so the
//! app can ask to save first or decline. It never force-quits and never
//! signals a process. Only the person's own instances are asked: an instance
//! another account runs is left alone, and so is Lumen itself.

#![allow(unsafe_code)]

use std::ffi::c_int;
use std::os::unix::fs::MetadataExt as _;
use std::path::Path;

use objc2_app_kit::NSRunningApplication;
use objc2_foundation::NSString;

use crate::probe::pid_info;

/// What asking an application to quit did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuitRequest {
    /// This many instances were asked to quit. Each may still ask to save, or
    /// decline; whether it quit is known only by looking again.
    Asked(usize),
    /// None of the person's instances of the bundle is running.
    NotRunning,
    /// The only instance found is Lumen itself; it was not asked.
    Itself,
    /// An instance was found but the request could not be sent, or the bundle
    /// could not be looked up.
    Failed,
}

/// The device and inode of `path`, following links as the bundle URL does.
fn identity(path: &Path) -> Option<(u64, u64)> {
    std::fs::metadata(path).ok().map(|m| (m.dev(), m.ino()))
}

/// The account that runs `pid`, from the kernel's process record.
fn owner(pid: c_int) -> Option<u32> {
    pid_info::<libc::proc_bsdinfo>(pid, libc::PROC_PIDTBSDINFO, 0).map(|info| info.pbi_uid)
}

/// Asks every instance of the application bundle at `bundle` (identifier
/// `bundle_id`) that the person runs to quit, the way its Quit menu item does.
///
/// An instance counts only when its bundle is the same folder as `bundle` (by
/// device and inode, so another copy with the same identifier is left alone),
/// it runs as the current account, and it is not this process.
#[must_use]
pub fn ask_to_quit(bundle_id: &str, bundle: &Path) -> QuitRequest {
    let Some(target) = identity(bundle) else {
        return QuitRequest::Failed;
    };
    // SAFETY: invariant: none. Why unsafe: `getuid` is an FFI call. Why it
    // holds: it takes no arguments, cannot fail and touches no memory.
    let me = unsafe { libc::getuid() };
    let this = std::process::id();
    let instances = NSRunningApplication::runningApplicationsWithBundleIdentifier(
        &NSString::from_str(bundle_id),
    )
    .to_vec();
    let (mut asked, mut itself, mut failed) = (0, false, false);
    for app in instances {
        let same_bundle = app
            .bundleURL()
            .and_then(|url| url.to_file_path())
            .and_then(|path| identity(&path))
            == Some(target);
        let pid = app.processIdentifier();
        if !same_bundle || owner(pid) != Some(me) {
            continue;
        }
        if u32::try_from(pid).is_ok_and(|pid| pid == this) {
            itself = true;
        } else if app.terminate() {
            asked += 1;
        } else {
            failed = true;
        }
    }
    match (asked, itself, failed) {
        (0, _, true) => QuitRequest::Failed,
        (0, true, false) => QuitRequest::Itself,
        (0, false, false) => QuitRequest::NotRunning,
        (n, _, _) => QuitRequest::Asked(n),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn a_bundle_that_is_not_there_is_a_failure() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("Missing.app");
        assert_eq!(
            ask_to_quit("dev.lumen.test.missing", &missing),
            QuitRequest::Failed
        );
    }

    #[test]
    fn an_app_nobody_runs_is_not_running() {
        let dir = tempfile::tempdir().unwrap();
        let bundle = dir.path().join("Nobody.app");
        std::fs::create_dir(&bundle).unwrap();
        assert_eq!(
            ask_to_quit("dev.lumen.test.nobody-runs-this", &bundle),
            QuitRequest::NotRunning
        );
    }

    #[test]
    fn the_owner_of_this_process_is_this_account() {
        let pid = c_int::try_from(std::process::id()).unwrap();
        // SAFETY: invariant: none. Why unsafe: FFI call. Why it holds: as above.
        assert_eq!(owner(pid), Some(unsafe { libc::getuid() }));
    }
}
