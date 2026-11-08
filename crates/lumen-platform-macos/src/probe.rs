//! The macOS [`InUseProbe`] on `libproc` (roadmap 6.8).
//!
//! For every running process it reads the open vnode file descriptors, the
//! working directory and the executable path, and reports the processes that use
//! the object: by identity (device and inode) for anything, and by canonical
//! path prefix for anything under a directory. The kernel reports open files in
//! the user-facing namespace (`/Users/...`) with symlinks resolved, so the target
//! directory is canonicalized too.
//!
//! An unprivileged process cannot read other users' file descriptors (measured on
//! macOS 26: 872 of 1,107 processes visible, all of them the user's own). Those
//! processes are counted as unseen and the answer is [`InUse::Partial`], never a
//! claim of non-use. Their executable paths can still be read (569 of 570
//! processes on macOS 26, 2026-11-07; the kernel has none), so a process of any
//! user that runs from inside a directory uses it, and
//! [`ProcessSnapshot::running`] can answer for every process.

#![allow(unsafe_code)]

use std::ffi::{c_int, c_void};
use std::mem::{MaybeUninit, size_of};
use std::os::unix::ffi::OsStrExt as _;
use std::os::unix::fs::MetadataExt as _;
use std::path::Path;

use lumen_application::ports::{InUse, InUseProbe};
use lumen_domain::{FileIdentity, PathFlavor, Process, RawPath, Timestamp, VolumeId};

/// `PROC_PIDFDVNODEPATHINFO` (`sys/proc_info.h`); not in `libc`.
const PROC_PIDFDVNODEPATHINFO: c_int = 2;

/// `struct proc_fileinfo` (`sys/proc_info.h`); not in `libc`. Field names match
/// the SDK so the mirror can be checked line by line.
#[repr(C)]
#[allow(clippy::struct_field_names, dead_code)]
#[derive(Debug, Clone, Copy)]
struct ProcFileInfo {
    fi_openflags: u32,
    fi_status: u32,
    fi_offset: i64,
    fi_type: i32,
    fi_guardflags: u32,
}

/// `struct vnode_fdinfowithpath` (`sys/proc_info.h`); not in `libc`.
#[repr(C)]
#[derive(Clone, Copy)]
struct VnodeFdInfoWithPath {
    pfi: ProcFileInfo,
    pvip: libc::vnode_info_path,
}

/// Asks the kernel which processes use an object.
#[derive(Debug, Clone, Copy, Default)]
pub struct ProcessProbe;

/// The kernel's 32-bit `dev_t`, from `MetadataExt::dev` (which sign-extends it).
fn low_32(dev: u64) -> u32 {
    u32::try_from(dev & 0xFFFF_FFFF).unwrap_or_default()
}

/// What to look for.
struct Target {
    device: u32,
    inode: u64,
    /// For a directory: everything inside it.
    under: Option<Folder>,
}

impl Target {
    fn matches(&self, device: u32, inode: u64, path: &[u8]) -> bool {
        (device == self.device && inode == self.inode)
            || self.under.as_ref().is_some_and(|dir| dir.holds(path))
    }
}

/// A directory, for asking whether a path the kernel reports lies inside it.
struct Folder {
    /// Its path followed by `/`.
    prefix: Vec<u8>,
    /// Its device (as the kernel reports it) and inode.
    identity: (u32, u64),
}

impl Folder {
    /// Whether `path` names something inside this folder.
    ///
    /// The kernel can spell a folder differently from the path Lumen holds: on a
    /// case-insensitive volume, `/Applications/foo.app` and `/Applications/Foo.app`
    /// (or two Unicode forms of one name) are the same folder. So a path whose
    /// ancestor at this folder's depth equals it ignoring ASCII case, or where both
    /// have non-ASCII bytes, is compared by identity: that ancestor is looked up
    /// and its device and inode decide. Anything else is a different folder, and
    /// costs no lookup.
    fn holds(&self, path: &[u8]) -> bool {
        if path.starts_with(&self.prefix) {
            return true;
        }
        let name = &self.prefix[..self.prefix.len().saturating_sub(1)];
        let slashes = self.prefix.split(|b| *b == b'/').count().saturating_sub(1);
        let Some(end) = path
            .iter()
            .enumerate()
            .filter(|(_, b)| **b == b'/')
            .nth(slashes.saturating_sub(1))
            .map(|(at, _)| at)
        else {
            return false;
        };
        let ancestor = &path[..end];
        let may_be_spelled =
            ancestor.eq_ignore_ascii_case(name) || (!ancestor.is_ascii() && !name.is_ascii());
        may_be_spelled
            && std::fs::metadata(Path::new(std::ffi::OsStr::from_bytes(ancestor)))
                .is_ok_and(|meta| (low_32(meta.dev()), meta.ino()) == self.identity)
    }
}

/// What one process revealed.
enum Seen {
    Uses(Process),
    Clean,
    Hidden,
}

/// Classifies an unavailable fd listing without consulting ambient errno.
fn fd_listing_failure(errno: Option<c_int>) -> Seen {
    if errno == Some(libc::ESRCH) {
        Seen::Clean
    } else {
        Seen::Hidden
    }
}

/// The process's thread-local errno, read immediately after a failed libproc call.
fn current_errno() -> c_int {
    // SAFETY: `__error` returns this thread's valid errno pointer on macOS.
    // Why unsafe: dereferencing the pointer returned by the libc accessor.
    // Why it holds: the accessor is defined to return the calling thread's errno.
    unsafe { *libc::__error() }
}

/// The bytes of a fixed-size C path buffer, up to its NUL.
fn c_path(path: &[[libc::c_char; 32]; 32]) -> Vec<u8> {
    path.iter()
        .flatten()
        .map(|c| c.to_ne_bytes()[0])
        .take_while(|b| *b != 0)
        .collect()
}

/// Calls `proc_pidinfo` for a fixed-size struct. `None` if the call fails or
/// returns a different size.
pub(crate) fn pid_info<T: Copy>(pid: c_int, flavor: c_int, arg: u64) -> Option<T> {
    let mut out = MaybeUninit::<T>::uninit();
    let size = c_int::try_from(size_of::<T>()).ok()?;
    // SAFETY: invariant: the kernel writes at most `size` bytes of `T` into
    // `out`. Why unsafe: FFI writing through a raw pointer. Why it holds: each
    // caller pairs `flavor` with the struct the header documents for it, and
    // `size` is exactly that struct's size.
    let written = unsafe { libc::proc_pidinfo(pid, flavor, arg, out.as_mut_ptr().cast(), size) };
    // SAFETY: invariant: `out` is fully initialised. Why unsafe: `assume_init`.
    // Why it holds: the kernel reported writing exactly `size` bytes; every field
    // is a plain integer or byte array.
    (written == size).then(|| unsafe { out.assume_init() })
}

/// The open file descriptors of `pid`, or the errno observed at a failed libproc call.
fn fds(pid: c_int) -> Result<Vec<libc::proc_fdinfo>, Option<c_int>> {
    // SAFETY: invariant: a null buffer of size 0 only queries the needed size.
    // Why unsafe: FFI call. Why it holds: documented sizing call.
    let needed =
        unsafe { libc::proc_pidinfo(pid, libc::PROC_PIDLISTFDS, 0, std::ptr::null_mut(), 0) };
    let errno = (needed < 0).then(current_errno);
    let needed = usize::try_from(needed)
        .ok()
        .filter(|n| *n > 0)
        .ok_or(errno)?;
    // Room for descriptors opened between the two calls.
    let capacity = needed / size_of::<libc::proc_fdinfo>() + 32;
    let mut list: Vec<libc::proc_fdinfo> = Vec::with_capacity(capacity);
    let bytes = c_int::try_from(capacity * size_of::<libc::proc_fdinfo>()).map_err(|_| None)?;
    // SAFETY: invariant: the kernel writes at most `bytes` bytes into the
    // vector's spare capacity. Why unsafe: FFI writing into uninitialised memory.
    // Why it holds: `bytes` is exactly the allocated capacity in bytes.
    let written = unsafe {
        libc::proc_pidinfo(
            pid,
            libc::PROC_PIDLISTFDS,
            0,
            list.as_mut_ptr().cast::<c_void>(),
            bytes,
        )
    };
    let errno = (written < 0).then(current_errno);
    let count = usize::try_from(written).map_err(|_| errno)? / size_of::<libc::proc_fdinfo>();
    // SAFETY: invariant: the first `count` elements are initialised. Why unsafe:
    // `set_len`. Why it holds: the kernel wrote `written` bytes, which is
    // `count` whole `proc_fdinfo` records, and `count <= capacity`.
    unsafe { list.set_len(count.min(capacity)) };
    Ok(list)
}

fn vnode_of(pid: c_int, fd: i32) -> Option<VnodeFdInfoWithPath> {
    let mut info = MaybeUninit::<VnodeFdInfoWithPath>::uninit();
    let size = c_int::try_from(size_of::<VnodeFdInfoWithPath>()).ok()?;
    // SAFETY: invariant: the kernel writes at most `size` bytes of
    // `vnode_fdinfowithpath` into `info`. Why unsafe: FFI writing through a raw
    // pointer. Why it holds: `VnodeFdInfoWithPath` mirrors the SDK struct (size
    // pinned by a test) and `size` is its size.
    let written = unsafe {
        libc::proc_pidfdinfo(
            pid,
            fd,
            PROC_PIDFDVNODEPATHINFO,
            info.as_mut_ptr().cast(),
            size,
        )
    };
    // SAFETY: invariant: fully initialised. Why unsafe: `assume_init`. Why it
    // holds: the kernel wrote exactly `size` bytes of plain-old-data fields.
    (written == size).then(|| unsafe { info.assume_init() })
}

/// A process's executable path, as far as the kernel gives it.
///
/// Unlike its open files, any process's executable path can be read without
/// privileges (measured on macOS 26, 2026-11-07: 569 of 570 processes, the
/// exception being the kernel, pid 0, which has none).
enum Exe {
    /// Its path.
    At(Vec<u8>),
    /// It has no path: it exited while being read (`ESRCH`), it is the
    /// kernel, or its executable file was deleted after it started (`ENOENT`,
    /// seen on a CI runner for a root job run from a removed temporary file).
    /// A deleted file lies in no folder, so it is inside none.
    Missing,
    /// It could not be read, with the errno observed (kept for diagnosis).
    Unreadable(#[cfg_attr(not(test), expect(dead_code, reason = "read by tests"))] Option<c_int>),
}

impl Exe {
    fn path(&self) -> Option<&[u8]> {
        match self {
            Self::At(path) => Some(path),
            Self::Missing | Self::Unreadable(_) => None,
        }
    }
}

fn executable(pid: c_int) -> Exe {
    let mut buffer = vec![0_u8; 4 * 1024];
    let Ok(size) = u32::try_from(buffer.len()) else {
        return Exe::Unreadable(None);
    };
    // SAFETY: invariant: the kernel writes at most `size` bytes. Why unsafe: FFI
    // writing through a raw pointer. Why it holds: `size` is the buffer length.
    let written = unsafe { libc::proc_pidpath(pid, buffer.as_mut_ptr().cast(), size) };
    let errno = (written <= 0).then(current_errno);
    match usize::try_from(written).ok().filter(|n| *n > 0) {
        Some(length) => {
            buffer.truncate(length);
            Exe::At(buffer)
        }
        None if matches!(errno, Some(libc::ESRCH | libc::ENOENT)) => Exe::Missing,
        None => Exe::Unreadable(errno),
    }
}

/// An object a process uses, as the kernel reports it.
struct Object {
    device: u32,
    inode: u64,
    path: Vec<u8>,
}

/// A readable process: who it is and everything it uses.
struct Visible {
    process: Process,
    exe: Exe,
    /// Its executable's device and inode, if they could be read.
    exe_identity: Option<(u32, u64)>,
    /// The working directory and every open vnode.
    objects: Vec<Object>,
}

/// One process as read from the kernel: what it uses, or why it could not be
/// read.
enum Observed {
    /// Its executable, working directory and open files.
    Visible(Box<Visible>),
    /// It exited while being read: it uses nothing.
    Gone,
    /// Its file descriptors could not be read (another user's process); only its
    /// pid and executable path are known.
    Hidden(c_int, Exe),
}

fn observe(pid: c_int) -> Observed {
    let open = match fds(pid) {
        Ok(open) => open,
        Err(errno) => {
            return match fd_listing_failure(errno) {
                Seen::Clean => Observed::Gone,
                _ => Observed::Hidden(pid, executable(pid)),
            };
        }
    };
    let exe = executable(pid);
    let exe_identity = exe.path().and_then(|path| {
        std::fs::metadata(Path::new(std::ffi::OsStr::from_bytes(path)))
            .ok()
            .map(|m| (low_32(m.dev()), m.ino()))
    });
    let mut objects = Vec::new();
    if let Some(info) = pid_info::<libc::proc_vnodepathinfo>(pid, libc::PROC_PIDVNODEPATHINFO, 0) {
        let vi = info.pvi_cdir.vip_vi.vi_stat;
        objects.push(Object {
            device: vi.vst_dev,
            inode: vi.vst_ino,
            path: c_path(&info.pvi_cdir.vip_path),
        });
    }
    objects.extend(
        open.iter()
            .filter(|fd| i32::try_from(fd.proc_fdtype).ok() == Some(libc::PROX_FDTYPE_VNODE))
            .filter_map(|fd| vnode_of(pid, fd.proc_fd))
            .map(|info| {
                let st = info.pvip.vip_vi.vi_stat;
                Object {
                    device: st.vst_dev,
                    inode: st.vst_ino,
                    path: c_path(&info.pvip.vip_path),
                }
            }),
    );
    let started_at =
        pid_info::<libc::proc_bsdinfo>(pid, libc::PROC_PIDTBSDINFO, 0).and_then(|bsd| {
            let micros = u32::try_from(bsd.pbi_start_tvusec).ok()?;
            Timestamp::from_unix(
                i64::try_from(bsd.pbi_start_tvsec).ok()?,
                micros.checked_mul(1000)?,
            )
            .ok()
        });
    Observed::Visible(Box::new(Visible {
        process: Process {
            pid: u32::try_from(pid).unwrap_or_default(),
            started_at,
            executable: exe
                .path()
                .and_then(|path| RawPath::from_unix_bytes(path.to_vec()).ok()),
            executable_identity: None,
            application: None,
        },
        exe,
        exe_identity,
        objects,
    }))
}

impl Observed {
    /// Whether this process uses `target`.
    fn seen(&self, target: &Target) -> Seen {
        match self {
            Self::Gone => Seen::Clean,
            // Its open files are unknown, but a process running from inside the
            // target uses it.
            Self::Hidden(pid, exe) => match exe.path() {
                Some(path) if target.under.as_ref().is_some_and(|dir| dir.holds(path)) => {
                    Seen::Uses(Process {
                        pid: u32::try_from(*pid).unwrap_or_default(),
                        started_at: None,
                        executable: RawPath::from_unix_bytes(path.to_vec()).ok(),
                        executable_identity: None,
                        application: None,
                    })
                }
                _ => Seen::Hidden,
            },
            Self::Visible(visible) => {
                let Visible {
                    process,
                    exe,
                    exe_identity,
                    objects,
                } = visible.as_ref();
                let exe_matches = exe
                    .path()
                    .is_some_and(|path| target.under.as_ref().is_some_and(|dir| dir.holds(path)))
                    || *exe_identity == Some((target.device, target.inode));
                let uses = exe_matches
                    || objects
                        .iter()
                        .any(|object| target.matches(object.device, object.inode, &object.path));
                if uses {
                    Seen::Uses(process.clone())
                } else {
                    Seen::Clean
                }
            }
        }
    }
}

fn all_pids() -> Option<Vec<c_int>> {
    // SAFETY: invariant: a null buffer only queries the count. Why unsafe: FFI.
    // Why it holds: documented sizing call.
    let count = unsafe { libc::proc_listallpids(std::ptr::null_mut(), 0) };
    let capacity = usize::try_from(count).ok()? + 64;
    let mut pids = vec![0 as c_int; capacity];
    let bytes = c_int::try_from(capacity * size_of::<c_int>()).ok()?;
    // SAFETY: invariant: the kernel writes at most `bytes` bytes of pids. Why
    // unsafe: FFI writing through a raw pointer. Why it holds: `bytes` is the
    // buffer's size.
    let written = unsafe { libc::proc_listallpids(pids.as_mut_ptr().cast(), bytes) };
    pids.truncate(usize::try_from(written).ok()?.min(capacity));
    Some(pids)
}

/// What to look for at `path`, which must still be `expected`; the answer to
/// give instead when it cannot be established.
fn target(path: &RawPath, expected: &FileIdentity) -> Result<Target, InUse> {
    if path.flavor() != PathFlavor::Unix {
        return Err(InUse::Unknown {
            reason: "not a Unix path",
        });
    }
    let native = Path::new(std::ffi::OsStr::from_bytes(path.as_bytes()));
    let Ok(meta) = std::fs::symlink_metadata(native) else {
        return Err(InUse::Unknown {
            reason: "the object cannot be observed",
        });
    };
    // Same identity rule as the enumerators (`MetadataExt::dev` sign-extends).
    let identity_matches = VolumeId::new(format!("dev:{:x}", meta.dev()))
        .is_ok_and(|volume| volume == expected.volume)
        && u128::from(meta.ino()) == expected.file.get();
    if !identity_matches {
        return Err(InUse::Unknown {
            reason: "the object changed since it was observed",
        });
    }
    let under = meta.is_dir().then(|| {
        std::fs::canonicalize(native).ok().map(|dir| {
            let mut prefix = dir.as_os_str().as_bytes().to_vec();
            if !prefix.ends_with(b"/") {
                prefix.push(b'/');
            }
            prefix
        })
    });
    let Some(under) = under.map_or(Some(None), |p| p.map(Some)) else {
        return Err(InUse::Unknown {
            reason: "the directory cannot be resolved",
        });
    };
    let identity = (low_32(meta.dev()), meta.ino());
    Ok(Target {
        device: identity.0,
        inode: identity.1,
        under: under.map(|prefix| Folder { prefix, identity }),
    })
}

/// The answer from what every process revealed.
fn answer(seen: impl Iterator<Item = Seen>) -> InUse {
    let mut users = Vec::new();
    let mut unseen = 0_u32;
    for one in seen {
        match one {
            Seen::Uses(process) => users.push(process),
            Seen::Clean => {}
            Seen::Hidden => unseen = unseen.saturating_add(1),
        }
    }
    if !users.is_empty() {
        InUse::Yes(users)
    } else if unseen > 0 {
        InUse::Partial {
            unseen: Some(unseen),
        }
    } else {
        InUse::No
    }
}

/// The process list could not be read: nothing is known.
const NO_PROCESS_LIST: InUse = InUse::Unknown {
    reason: "the process list cannot be read",
};

impl InUseProbe for ProcessProbe {
    fn probe(&self, path: &RawPath, expected: &FileIdentity) -> InUse {
        let target = match target(path, expected) {
            Ok(target) => target,
            Err(unknown) => return unknown,
        };
        let Some(pids) = all_pids() else {
            return NO_PROCESS_LIST;
        };
        answer(pids.into_iter().map(|pid| observe(pid).seen(&target)))
    }
}

impl ProcessProbe {
    /// Reads every process once, for answering many questions at one moment:
    /// an assessment's candidates. Its answers describe that moment; moving
    /// and removing ask the live [`ProcessProbe`] again, item by item.
    #[must_use]
    pub fn snapshot(self) -> ProcessSnapshot {
        ProcessSnapshot {
            processes: all_pids().map(|pids| pids.into_iter().map(observe).collect()),
        }
    }
}

/// The process table read once by [`ProcessProbe::snapshot`]; answers by the
/// same rules as [`ProcessProbe`].
pub struct ProcessSnapshot {
    /// `None` if the process list could not be read.
    processes: Option<Vec<Observed>>,
}

impl std::fmt::Debug for ProcessSnapshot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProcessSnapshot")
            .field("processes", &self.processes.as_ref().map(Vec::len))
            .finish()
    }
}

impl ProcessSnapshot {
    /// Whether a process runs an executable inside the application bundle at
    /// `bundle` (ADR-0059, ADR-0062): `Some(true)` when any process does, whoever
    /// it belongs to; `Some(false)` only when every process's executable path was
    /// read and none lies inside; `None` when that cannot be known, or the bundle
    /// cannot be looked up.
    ///
    /// Only executable paths are compared, never open files, so a process that
    /// hides its open files from Lumen (another user's) still answers here.
    #[must_use]
    pub fn running(&self, bundle: &RawPath) -> Option<bool> {
        if bundle.flavor() != PathFlavor::Unix {
            return None;
        }
        let meta =
            std::fs::symlink_metadata(Path::new(std::ffi::OsStr::from_bytes(bundle.as_bytes())))
                .ok()?;
        let mut prefix = bundle.as_bytes().to_vec();
        if !prefix.ends_with(b"/") {
            prefix.push(b'/');
        }
        let folder = Folder {
            prefix,
            identity: (low_32(meta.dev()), meta.ino()),
        };
        let processes = self.processes.as_ref()?;
        let mut unreadable = false;
        for process in processes {
            let exe = match process {
                Observed::Visible(visible) => &visible.exe,
                Observed::Hidden(_, exe) => exe,
                Observed::Gone => continue,
            };
            match exe {
                Exe::At(path) if folder.holds(path) => return Some(true),
                Exe::At(_) | Exe::Missing => {}
                Exe::Unreadable(_) => unreadable = true,
            }
        }
        (!unreadable).then_some(false)
    }
}

#[cfg(test)]
impl ProcessSnapshot {
    /// The processes whose executable path could not be read: pid and errno.
    fn unreadable(&self) -> Vec<(c_int, Option<c_int>)> {
        self.processes
            .iter()
            .flatten()
            .filter_map(|process| match process {
                Observed::Visible(visible) => match visible.exe {
                    Exe::Unreadable(errno) => {
                        Some((c_int::try_from(visible.process.pid).unwrap_or(-1), errno))
                    }
                    _ => None,
                },
                Observed::Hidden(pid, Exe::Unreadable(errno)) => Some((*pid, *errno)),
                _ => None,
            })
            .collect()
    }
}

impl InUseProbe for ProcessSnapshot {
    fn probe(&self, path: &RawPath, expected: &FileIdentity) -> InUse {
        let target = match target(path, expected) {
            Ok(target) => target,
            Err(unknown) => return unknown,
        };
        let Some(processes) = &self.processes else {
            return NO_PROCESS_LIST;
        };
        answer(processes.iter().map(|process| process.seen(&target)))
    }
}

#[cfg(test)]
mod tests {
    use std::mem::offset_of;

    use std::path::PathBuf;

    use lumen_domain::{FileId, RawPath};

    use super::*;

    #[test]
    fn fd_listing_failure_only_treats_captured_esrch_as_clean() {
        assert!(matches!(fd_listing_failure(Some(libc::ESRCH)), Seen::Clean));
        assert!(matches!(
            fd_listing_failure(Some(libc::EACCES)),
            Seen::Hidden
        ));
        assert!(matches!(fd_listing_failure(None), Seen::Hidden));
    }

    #[test]
    fn the_mirrored_struct_matches_the_sdk() {
        // sizeof / offsetof measured with the macOS 26 SDK.
        assert_eq!(size_of::<ProcFileInfo>(), 24);
        assert_eq!(size_of::<libc::vnode_info_path>(), 1176);
        assert_eq!(size_of::<VnodeFdInfoWithPath>(), 1200);
        assert_eq!(offset_of!(VnodeFdInfoWithPath, pvip), 24);
    }

    fn identity(path: &Path) -> Result<(RawPath, FileIdentity), Box<dyn std::error::Error>> {
        let meta = std::fs::symlink_metadata(path)?;
        let raw = RawPath::from_unix_bytes(path.as_os_str().as_bytes().to_vec())?;
        Ok((
            raw,
            FileIdentity {
                volume: VolumeId::new(format!("dev:{:x}", meta.dev()))?,
                file: FileId::new(u128::from(meta.ino())),
            },
        ))
    }

    fn me() -> u32 {
        std::process::id()
    }

    #[test]
    fn an_open_file_and_its_directory_are_in_use_by_this_process()
    -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let file = dir.path().join("held");
        std::fs::write(&file, b"x")?;
        let _held = std::fs::File::open(&file)?;
        let snapshot = ProcessProbe.snapshot();
        for probe in [&ProcessProbe as &dyn InUseProbe, &snapshot] {
            for target in [file.as_path(), dir.path()] {
                let (raw, id) = identity(target)?;
                let answer = probe.probe(&raw, &id);
                assert!(
                    matches!(&answer, InUse::Yes(users) if users.iter().any(|p| p.pid == me())),
                    "{target:?}: {answer:?}"
                );
            }
        }
        Ok(())
    }

    #[test]
    fn this_test_runs_under_its_own_folder_and_nothing_else_is_claimed()
    -> Result<(), Box<dyn std::error::Error>> {
        let exe = std::fs::canonicalize(std::env::current_exe()?)?;
        let folder = exe.parent().ok_or("the test has a folder")?;
        let raw = |path: &Path| RawPath::from_unix_bytes(path.as_os_str().as_bytes().to_vec());
        let snapshot = ProcessProbe.snapshot();
        assert_eq!(snapshot.running(&raw(folder)?), Some(true));
        let elsewhere = tempfile::tempdir()?;
        assert_eq!(
            snapshot.running(&raw(elsewhere.path())?),
            Some(false),
            "every executable path is readable, and none is in an empty folder; unreadable: {:?}",
            snapshot
                .unreadable()
                .into_iter()
                .map(|(pid, errno)| {
                    let name = std::process::Command::new("/bin/ps")
                        .args(["-o", "user=,comm=", "-p", &pid.to_string()])
                        .output()
                        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
                        .unwrap_or_default();
                    (pid, errno, name)
                })
                .collect::<Vec<_>>()
        );
        Ok(())
    }

    #[test]
    fn a_process_whose_files_are_hidden_is_still_seen_by_its_executable()
    -> Result<(), Box<dyn std::error::Error>> {
        // launchd (pid 1) runs as root: unless the test does too, its open files
        // cannot be read, but its executable path can.
        let sbin = RawPath::from_unix_bytes(b"/sbin".to_vec())?;
        let snapshot = ProcessProbe.snapshot();
        assert_eq!(snapshot.running(&sbin), Some(true));
        let (raw, id) = identity(Path::new("/sbin"))?;
        assert!(matches!(ProcessProbe.probe(&raw, &id), InUse::Yes(_)));
        Ok(())
    }

    /// A folder named `name` in a fresh temporary directory, and whether its
    /// volume ignores letter case.
    fn folder(
        name: &str,
    ) -> Result<(tempfile::TempDir, PathBuf, bool), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let path = std::fs::canonicalize(dir.path())?.join(name);
        std::fs::create_dir_all(path.join("Contents/MacOS"))?;
        let shouted = path.with_file_name(name.to_ascii_uppercase());
        Ok((dir, path, shouted.exists()))
    }

    fn of(path: &Path) -> Result<Folder, Box<dyn std::error::Error>> {
        let meta = std::fs::symlink_metadata(path)?;
        let mut prefix = path.as_os_str().as_bytes().to_vec();
        prefix.push(b'/');
        Ok(Folder {
            prefix,
            identity: (low_32(meta.dev()), meta.ino()),
        })
    }

    #[test]
    fn a_folder_is_recognised_however_the_kernel_spells_it()
    -> Result<(), Box<dyn std::error::Error>> {
        let (_dir, path, ignores_case) = folder("Foo.app")?;
        let parent = path.parent().ok_or("a parent")?;
        std::fs::create_dir(parent.join("Foo.app.old"))?;
        let foo = of(&path)?;
        let inside = |name: &str| {
            let mut bytes = parent.join(name).as_os_str().as_bytes().to_vec();
            bytes.extend_from_slice(b"/Contents/MacOS/foo");
            bytes
        };
        assert!(foo.holds(&inside("Foo.app")));
        assert_eq!(foo.holds(&inside("fOO.APP")), ignores_case);
        assert!(
            !foo.holds(&inside("Foo.app.old")),
            "a longer name is another folder"
        );
        assert!(!foo.holds(&inside("Bar.app")));
        assert!(
            !foo.holds(path.as_os_str().as_bytes()),
            "the folder itself is not inside it"
        );
        Ok(())
    }

    #[test]
    fn an_app_started_by_another_spelling_of_its_path_is_running()
    -> Result<(), Box<dyn std::error::Error>> {
        use std::io::Write as _;
        use std::os::unix::fs::OpenOptionsExt as _;

        let (_dir, path, ignores_case) = folder("Sleeper.app")?;
        let binary = path.join("Contents/MacOS/sleeper");
        let mut out = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o755)
            .open(&binary)?;
        out.write_all(&std::fs::read("/bin/sleep")?)?;
        drop(out);
        let spelled = if ignores_case {
            path.with_file_name("sleeper.APP")
                .join("Contents/MacOS/sleeper")
        } else {
            binary.clone()
        };
        let mut child = std::process::Command::new(&spelled).arg("30").spawn()?;
        let bundle = RawPath::from_unix_bytes(path.as_os_str().as_bytes().to_vec())?;
        let mut running = None;
        // The child may need a moment before the kernel reports its new image.
        for _ in 0..50 {
            running = ProcessProbe.snapshot().running(&bundle);
            if running == Some(true) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        child.kill()?;
        child.wait()?;
        assert_eq!(running, Some(true), "started as {spelled:?}");
        Ok(())
    }

    #[test]
    fn a_snapshot_answers_for_the_moment_it_was_taken() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let file = dir.path().join("later");
        std::fs::write(&file, b"x")?;
        let before = ProcessProbe.snapshot();
        let _held = std::fs::File::open(&file)?;
        let (raw, id) = identity(&file)?;
        assert!(
            matches!(before.probe(&raw, &id), InUse::No | InUse::Partial { .. }),
            "opened after the snapshot"
        );
        assert!(matches!(ProcessProbe.probe(&raw, &id), InUse::Yes(_)));
        assert!(matches!(
            ProcessProbe.snapshot().probe(&raw, &id),
            InUse::Yes(_)
        ));
        Ok(())
    }

    #[test]
    fn an_unopened_file_is_never_claimed_in_use_or_provably_unused_without_full_visibility()
    -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let file = dir.path().join("idle");
        std::fs::write(&file, b"x")?;
        let (raw, id) = identity(&file)?;
        for answer in [
            ProcessProbe.probe(&raw, &id),
            ProcessProbe.snapshot().probe(&raw, &id),
        ] {
            assert!(
                matches!(answer, InUse::No | InUse::Partial { .. }),
                "{answer:?}"
            );
        }
        Ok(())
    }

    #[test]
    fn a_changed_object_is_unknown() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        std::fs::write(dir.path().join("a"), b"1")?;
        std::fs::write(dir.path().join("b"), b"2")?;
        let (_, a) = identity(&dir.path().join("a"))?;
        let (b, _) = identity(&dir.path().join("b"))?;
        assert!(matches!(ProcessProbe.probe(&b, &a), InUse::Unknown { .. }));
        assert!(matches!(
            ProcessProbe.snapshot().probe(&b, &a),
            InUse::Unknown { .. }
        ));
        let gone =
            RawPath::from_unix_bytes(dir.path().join("gone").as_os_str().as_bytes().to_vec())?;
        assert!(matches!(
            ProcessProbe.probe(&gone, &a),
            InUse::Unknown { .. }
        ));
        Ok(())
    }
}
