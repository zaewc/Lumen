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
//! claim of non-use.

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
    /// For a directory: its canonical path followed by `/`.
    under: Option<Vec<u8>>,
}

impl Target {
    fn matches(&self, device: u32, inode: u64, path: &[u8]) -> bool {
        (device == self.device && inode == self.inode)
            || self
                .under
                .as_deref()
                .is_some_and(|dir| path.starts_with(dir))
    }
}

/// What one process revealed.
enum Seen {
    Uses(Process),
    Clean,
    Hidden,
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
fn pid_info<T: Copy>(pid: c_int, flavor: c_int, arg: u64) -> Option<T> {
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

/// The open file descriptors of `pid`, or `None` if they cannot be read.
fn fds(pid: c_int) -> Option<Vec<libc::proc_fdinfo>> {
    // SAFETY: invariant: a null buffer of size 0 only queries the needed size.
    // Why unsafe: FFI call. Why it holds: documented sizing call.
    let needed =
        unsafe { libc::proc_pidinfo(pid, libc::PROC_PIDLISTFDS, 0, std::ptr::null_mut(), 0) };
    let needed = usize::try_from(needed).ok().filter(|n| *n > 0)?;
    // Room for descriptors opened between the two calls.
    let capacity = needed / size_of::<libc::proc_fdinfo>() + 32;
    let mut list: Vec<libc::proc_fdinfo> = Vec::with_capacity(capacity);
    let bytes = c_int::try_from(capacity * size_of::<libc::proc_fdinfo>()).ok()?;
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
    let count = usize::try_from(written).ok()? / size_of::<libc::proc_fdinfo>();
    // SAFETY: invariant: the first `count` elements are initialised. Why unsafe:
    // `set_len`. Why it holds: the kernel wrote `written` bytes, which is
    // `count` whole `proc_fdinfo` records, and `count <= capacity`.
    unsafe { list.set_len(count.min(capacity)) };
    Some(list)
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

fn executable(pid: c_int) -> Option<Vec<u8>> {
    let mut buffer = vec![0_u8; 4 * 1024];
    let size = u32::try_from(buffer.len()).ok()?;
    // SAFETY: invariant: the kernel writes at most `size` bytes. Why unsafe: FFI
    // writing through a raw pointer. Why it holds: `size` is the buffer length.
    let written = unsafe { libc::proc_pidpath(pid, buffer.as_mut_ptr().cast(), size) };
    buffer.truncate(usize::try_from(written).ok().filter(|n| *n > 0)?);
    Some(buffer)
}

fn inspect(pid: c_int, target: &Target) -> Seen {
    let Some(open) = fds(pid) else {
        // A process that exited cannot use anything; one we may not inspect can.
        return if std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH) {
            Seen::Clean
        } else {
            Seen::Hidden
        };
    };
    let exe = executable(pid);
    let exe_matches = exe.as_deref().is_some_and(|path| {
        target
            .under
            .as_deref()
            .is_some_and(|dir| path.starts_with(dir))
            || std::fs::metadata(Path::new(std::ffi::OsStr::from_bytes(path)))
                .is_ok_and(|m| low_32(m.dev()) == target.device && m.ino() == target.inode)
    });
    let cwd_matches = pid_info::<libc::proc_vnodepathinfo>(pid, libc::PROC_PIDVNODEPATHINFO, 0)
        .is_some_and(|info| {
            let vi = info.pvi_cdir.vip_vi.vi_stat;
            target.matches(vi.vst_dev, vi.vst_ino, &c_path(&info.pvi_cdir.vip_path))
        });
    let fd_matches = open
        .iter()
        .filter(|fd| i32::try_from(fd.proc_fdtype).ok() == Some(libc::PROX_FDTYPE_VNODE))
        .filter_map(|fd| vnode_of(pid, fd.proc_fd))
        .any(|info| {
            let st = info.pvip.vip_vi.vi_stat;
            target.matches(st.vst_dev, st.vst_ino, &c_path(&info.pvip.vip_path))
        });
    if !(exe_matches || cwd_matches || fd_matches) {
        return Seen::Clean;
    }
    let started_at =
        pid_info::<libc::proc_bsdinfo>(pid, libc::PROC_PIDTBSDINFO, 0).and_then(|bsd| {
            let micros = u32::try_from(bsd.pbi_start_tvusec).ok()?;
            Timestamp::from_unix(
                i64::try_from(bsd.pbi_start_tvsec).ok()?,
                micros.checked_mul(1000)?,
            )
            .ok()
        });
    Seen::Uses(Process {
        pid: u32::try_from(pid).unwrap_or_default(),
        started_at,
        executable: exe.and_then(|path| RawPath::from_unix_bytes(path).ok()),
        executable_identity: None,
        application: None,
    })
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

impl InUseProbe for ProcessProbe {
    fn probe(&self, path: &RawPath, expected: &FileIdentity) -> InUse {
        if path.flavor() != PathFlavor::Unix {
            return InUse::Unknown {
                reason: "not a Unix path",
            };
        }
        let native = Path::new(std::ffi::OsStr::from_bytes(path.as_bytes()));
        let Ok(meta) = std::fs::symlink_metadata(native) else {
            return InUse::Unknown {
                reason: "the object cannot be observed",
            };
        };
        // Same identity rule as the enumerators (`MetadataExt::dev` sign-extends).
        let identity_matches = VolumeId::new(format!("dev:{:x}", meta.dev()))
            .is_ok_and(|volume| volume == expected.volume)
            && u128::from(meta.ino()) == expected.file.get();
        if !identity_matches {
            return InUse::Unknown {
                reason: "the object changed since it was observed",
            };
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
            return InUse::Unknown {
                reason: "the directory cannot be resolved",
            };
        };
        let target = Target {
            device: low_32(meta.dev()),
            inode: meta.ino(),
            under,
        };
        let Some(pids) = all_pids() else {
            return InUse::Unknown {
                reason: "the process list cannot be read",
            };
        };
        let mut users = Vec::new();
        let mut unseen = 0_u32;
        for pid in pids {
            match inspect(pid, &target) {
                Seen::Uses(process) => users.push(process),
                Seen::Clean => {}
                Seen::Hidden => unseen = unseen.saturating_add(1),
            }
        }
        if !users.is_empty() {
            InUse::Yes(users)
        } else if unseen > 0 {
            InUse::Partial { unseen }
        } else {
            InUse::No
        }
    }
}

#[cfg(test)]
mod tests {
    use std::mem::offset_of;

    use lumen_domain::{FileId, RawPath};

    use super::*;

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
        for target in [file.as_path(), dir.path()] {
            let (raw, id) = identity(target)?;
            let answer = ProcessProbe.probe(&raw, &id);
            assert!(
                matches!(&answer, InUse::Yes(users) if users.iter().any(|p| p.pid == me())),
                "{target:?}: {answer:?}"
            );
        }
        Ok(())
    }

    #[test]
    fn an_unopened_file_is_never_claimed_in_use_or_provably_unused_without_full_visibility()
    -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let file = dir.path().join("idle");
        std::fs::write(&file, b"x")?;
        let (raw, id) = identity(&file)?;
        let answer = ProcessProbe.probe(&raw, &id);
        assert!(
            matches!(answer, InUse::No | InUse::Partial { .. }),
            "{answer:?}"
        );
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
        let gone =
            RawPath::from_unix_bytes(dir.path().join("gone").as_os_str().as_bytes().to_vec())?;
        assert!(matches!(
            ProcessProbe.probe(&gone, &a),
            InUse::Unknown { .. }
        ));
        Ok(())
    }
}
