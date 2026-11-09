//! `lumen-helper`: Lumen's opt-in privileged helper (ADR-0063).
//!
//! On macOS it is a launch daemon that the desktop app registers with
//! `SMAppService` the first time the person chooses something that needs it.
//! launchd starts it on demand when Lumen connects to its Mach service, and it
//! exits after a minute without requests, so it runs only while it is used.
//!
//! It answers only Lumen signed with a Developer ID by the same team as
//! itself, checked by XPC on every message; it reads who is asking from the
//! connection, never from the request; and it decides every request itself
//! ([`lumen_application::helper::answer`]). This first step (roadmap 16.3b)
//! answers `status` only: it moves, deletes and opens nothing.

#![allow(
    clippy::print_stderr,
    reason = "a launch daemon's stderr goes to the system log; it is the helper's only log"
)]

use std::process::ExitCode;

fn main() -> ExitCode {
    #[cfg(target_os = "macos")]
    {
        mac::run()
    }
    #[cfg(not(target_os = "macos"))]
    {
        eprintln!("lumen-helper: only macOS has a helper so far");
        ExitCode::FAILURE
    }
}

#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
mod mac {
    //! The daemon's lifetime: check in on the Mach service, answer, idle-exit.

    use std::process::ExitCode;
    use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
    use std::sync::{Arc, OnceLock};
    use std::time::{Duration, Instant};

    use std::collections::BTreeMap;

    use lumen_application::helper::HelperHost;
    use lumen_application::helper::gate::HelperEvidence;
    use lumen_application::ports::{ExecutionFailure, InUse, PreparedItem};
    use lumen_application::uninstall::BundleEvidence;
    use lumen_domain::{
        FileIdentity, ItemLocation, OperationId, QuarantineFailure, RawPath, Timestamp,
    };
    use lumen_platform_macos::helper::serve;
    use lumen_platform_macos::xpc::Peer;

    /// How long the helper waits without a request before it exits. launchd
    /// throttles a job that ran less than 10 seconds, so a minute keeps the
    /// next start prompt (research 37).
    const IDLE: Duration = Duration::from_secs(60);

    /// How often the helper checks whether it is idle.
    const TICK: Duration = Duration::from_secs(5);

    /// Requests being answered, and when the last one started (seconds since
    /// the helper started).
    struct Activity {
        started: OnceLock<Instant>,
        busy: AtomicUsize,
        last: AtomicU64,
    }

    impl Activity {
        fn now(&self) -> u64 {
            self.started.get_or_init(Instant::now).elapsed().as_secs()
        }

        fn idle(&self) -> bool {
            self.busy.load(Ordering::SeqCst) == 0
                && self.now().saturating_sub(self.last.load(Ordering::SeqCst)) >= IDLE.as_secs()
        }
    }

    /// The computer as this step of the helper sees it: nothing yet. Every
    /// `quarantine` request is refused before anything is looked at (no
    /// caller counts as an administrator), and nothing is moved. The next
    /// step gathers the evidence as root and moves through `lumen-fs-exec`.
    struct Unequipped;

    impl HelperHost for Unequipped {
        fn administrator(&self, _uid: u32) -> Option<bool> {
            None
        }

        fn observe(&self, _caller: u32, path: &RawPath) -> HelperEvidence {
            HelperEvidence {
                bundle: BundleEvidence {
                    path: path.clone(),
                    folders: Vec::new(),
                    directory: None,
                    owned_by_user: None,
                    linked_services: None,
                    helpers: BTreeMap::new(),
                    running: None,
                    store_on_volume: None,
                },
                identity: None,
                owner: None,
                restricted: None,
            }
        }

        fn prepare(
            &self,
            _root: &RawPath,
            _relative: &[Vec<u8>],
        ) -> Result<(PreparedItem, FileIdentity), ExecutionFailure> {
            Err(UNEQUIPPED)
        }

        fn in_use(&self, _path: &RawPath, _identity: &FileIdentity) -> InUse {
            InUse::Unknown {
                reason: "this helper does not read the process list yet",
            }
        }

        fn quarantine(
            &self,
            _caller: u32,
            _item: &PreparedItem,
            _operation: OperationId,
            _record: &[u8],
        ) -> Result<(), ExecutionFailure> {
            Err(UNEQUIPPED)
        }

        fn now(&self) -> Timestamp {
            Timestamp::UNIX_EPOCH
        }
    }

    /// Nothing was moved.
    const UNEQUIPPED: ExecutionFailure = ExecutionFailure {
        reason: QuarantineFailure::VolumeUnsupported,
        left: ItemLocation::Source,
    };

    pub(super) fn run() -> ExitCode {
        // SAFETY: invariant: none. Why unsafe: `geteuid` is an FFI call. Why it
        // holds: it takes no arguments, cannot fail and touches no memory.
        if unsafe { libc::geteuid() } != 0 {
            eprintln!("lumen-helper: runs only as Lumen's launch daemon, started by launchd");
            return ExitCode::FAILURE;
        }
        let activity = Arc::new(Activity {
            started: OnceLock::new(),
            busy: AtomicUsize::new(0),
            last: AtomicU64::new(0),
        });
        activity.started.get_or_init(Instant::now);
        let seen = Arc::clone(&activity);
        let listener = serve(Arc::new(move |peer: Peer, body: &str| {
            seen.busy.fetch_add(1, Ordering::SeqCst);
            seen.last.store(seen.now(), Ordering::SeqCst);
            let answer = lumen_application::helper::answer(
                body,
                peer.euid,
                env!("CARGO_PKG_VERSION"),
                &Unequipped,
            );
            seen.busy.fetch_sub(1, Ordering::SeqCst);
            answer
        }));
        let listener = match listener {
            Ok(listener) => listener,
            Err(error) => {
                eprintln!("lumen-helper: {error}");
                return ExitCode::FAILURE;
            }
        };
        while !activity.idle() {
            std::thread::sleep(TICK);
        }
        drop(listener);
        ExitCode::SUCCESS
    }
}
