//! The only `unsafe` code in the macOS adapter: three `getattrlist`-family system
//! calls that fill a caller-owned buffer, and the I/O policy calls behind
//! [`NoMaterialization`]. Everything the system calls return is parsed by the
//! safe, bounds-checked [`attrbuf`](crate::attrbuf) module.
//!
//! All calls request `FSOPT_ATTR_CMN_EXTENDED` (the fork group carries
//! `ATTR_CMNEXT_*`) and never `FSOPT_PACK_INVAL_ATTRS` (ADR-0028's research note).
//! Lookups add `FSOPT_NOFOLLOW`: a final symlink is described, never followed.

#![allow(unsafe_code)]

use std::ffi::{CStr, c_int, c_ulong};
use std::fs::File;
use std::io;
use std::os::fd::AsRawFd as _;

use crate::attrbuf::AttributeSet;

// `sys/resource.h` (macOS 26 SDK); `libc` does not bind these.
const IOPOL_TYPE_VFS_MATERIALIZE_DATALESS_FILES: c_int = 3;
const IOPOL_SCOPE_THREAD: c_int = 1;
const IOPOL_MATERIALIZE_DATALESS_FILES_OFF: c_int = 1;
const IOPOL_MATERIALIZE_DATALESS_FILES_BASIC_MASK: c_int = 3;

unsafe extern "C" {
    fn getiopolicy_np(iotype: c_int, scope: c_int) -> c_int;
    fn setiopolicy_np(iotype: c_int, scope: c_int, policy: c_int) -> c_int;
}

/// The current thread's dataless-file materialization policy, or `None` if it
/// cannot be read.
pub(crate) fn thread_materialization() -> Option<c_int> {
    // SAFETY: invariant: none; the call takes two integers and touches no memory
    // of ours. Why unsafe: FFI call. Why it holds: the arguments are the
    // documented constants for this iotype and scope.
    let policy = unsafe {
        getiopolicy_np(
            IOPOL_TYPE_VFS_MATERIALIZE_DATALESS_FILES,
            IOPOL_SCOPE_THREAD,
        )
    };
    (policy >= 0).then_some(policy)
}

fn set_thread_materialization(policy: c_int) -> io::Result<()> {
    // SAFETY: invariant: none; the call takes three integers and touches no memory
    // of ours. Why unsafe: FFI call. Why it holds: the iotype and scope are the
    // documented constants and `policy` is one of its documented values.
    let result = unsafe {
        setiopolicy_np(
            IOPOL_TYPE_VFS_MATERIALIZE_DATALESS_FILES,
            IOPOL_SCOPE_THREAD,
            policy,
        )
    };
    if result == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

/// While alive, the current thread cannot materialize (download) dataless files:
/// an access that would fetch their contents fails with `EDEADLK` instead. The
/// process policy is inherited from the parent process and may be `ON`, so this
/// is set per thread, which takes precedence, and restored on drop.
#[derive(Debug)]
pub(crate) struct NoMaterialization {
    previous: Option<c_int>,
}

impl NoMaterialization {
    /// Turns materialization off for this thread.
    ///
    /// # Errors
    ///
    /// Returns the OS error if the policy cannot be set; callers must then not
    /// touch the filesystem.
    pub(crate) fn enter() -> io::Result<Self> {
        let previous = thread_materialization();
        set_thread_materialization(IOPOL_MATERIALIZE_DATALESS_FILES_OFF)?;
        Ok(Self { previous })
    }
}

impl Drop for NoMaterialization {
    fn drop(&mut self) {
        if let Some(previous) = self.previous
            && set_thread_materialization(previous & IOPOL_MATERIALIZE_DATALESS_FILES_BASIC_MASK)
                .is_err()
        {
            // The thread keeps materialization off: the safe direction.
        }
    }
}

fn attrlist(set: AttributeSet) -> libc::attrlist {
    libc::attrlist {
        bitmapcount: libc::ATTR_BIT_MAP_COUNT,
        reserved: 0,
        commonattr: set.common,
        volattr: set.volume,
        dirattr: set.directory,
        fileattr: set.file,
        forkattr: set.extended,
    }
}

/// Retries a call interrupted by a signal; any other failure is returned.
fn retry(mut call: impl FnMut() -> c_int) -> io::Result<c_int> {
    loop {
        let result = call();
        if result >= 0 {
            return Ok(result);
        }
        let error = io::Error::last_os_error();
        if error.kind() != io::ErrorKind::Interrupted {
            return Err(error);
        }
    }
}

/// Reads the next batch of `dir`'s entries into `buffer` and returns how many it
/// holds; 0 means the listing is finished.
pub(crate) fn read_batch(dir: &File, set: AttributeSet, buffer: &mut [u8]) -> io::Result<usize> {
    let mut list = attrlist(set);
    let count = retry(|| {
        // SAFETY: invariant: the kernel writes at most `buffer.len()` bytes into
        // `buffer` and only reads `list`. Why unsafe: FFI call. Why it holds: the
        // descriptor is borrowed from a live `File`, `list` is a fully initialised
        // `attrlist` on this stack frame, and the pointer and length both come from
        // the same exclusive `&mut [u8]`, which outlives the call.
        unsafe {
            libc::getattrlistbulk(
                dir.as_raw_fd(),
                (&raw mut list).cast(),
                buffer.as_mut_ptr().cast(),
                buffer.len(),
                u64::from(libc::FSOPT_ATTR_CMN_EXTENDED),
            )
        }
    })?;
    usize::try_from(count).map_err(io::Error::other)
}

/// Reads the attributes of `name` inside `dir` into `buffer` (one group), without
/// following a final symlink.
pub(crate) fn lookup_at(
    dir: &File,
    name: &CStr,
    set: AttributeSet,
    buffer: &mut [u8],
) -> io::Result<()> {
    let mut list = attrlist(set);
    let options = c_ulong::from(libc::FSOPT_NOFOLLOW | libc::FSOPT_ATTR_CMN_EXTENDED);
    retry(|| {
        // SAFETY: invariant: the kernel reads the NUL-terminated `name` and `list`
        // and writes at most `buffer.len()` bytes into `buffer`. Why unsafe: FFI
        // call. Why it holds: `name` is a `&CStr` (terminated, no interior NUL),
        // the descriptor comes from a live `File`, `list` is initialised on this
        // frame, and the pointer and length come from one exclusive `&mut [u8]`.
        unsafe {
            libc::getattrlistat(
                dir.as_raw_fd(),
                name.as_ptr(),
                (&raw mut list).cast(),
                buffer.as_mut_ptr().cast(),
                buffer.len(),
                options,
            )
        }
    })
    .map(drop)
}

/// Reads the attributes of the object at `path` into `buffer` (one group),
/// without following a final symlink.
pub(crate) fn lookup(path: &CStr, set: AttributeSet, buffer: &mut [u8]) -> io::Result<()> {
    let mut list = attrlist(set);
    retry(|| {
        // SAFETY: invariant: the kernel reads the NUL-terminated `path` and `list`
        // and writes at most `buffer.len()` bytes into `buffer`. Why unsafe: FFI
        // call. Why it holds: `path` is a `&CStr`, `list` is initialised on this
        // frame, and the pointer and length come from one exclusive `&mut [u8]`.
        unsafe {
            libc::getattrlist(
                path.as_ptr(),
                (&raw mut list).cast(),
                buffer.as_mut_ptr().cast(),
                buffer.len(),
                libc::FSOPT_NOFOLLOW | libc::FSOPT_ATTR_CMN_EXTENDED,
            )
        }
    })
    .map(drop)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ON: c_int = 2;

    #[test]
    fn the_guard_turns_materialization_off_and_restores_the_thread_policy() -> io::Result<()> {
        std::thread::spawn(|| -> io::Result<()> {
            set_thread_materialization(ON)?;
            {
                let _guard = NoMaterialization::enter()?;
                assert_eq!(
                    thread_materialization(),
                    Some(IOPOL_MATERIALIZE_DATALESS_FILES_OFF)
                );
                {
                    let _nested = NoMaterialization::enter()?;
                }
                assert_eq!(
                    thread_materialization(),
                    Some(IOPOL_MATERIALIZE_DATALESS_FILES_OFF),
                    "a nested guard restores OFF, not the outer value"
                );
            }
            assert_eq!(thread_materialization(), Some(ON));
            Ok(())
        })
        .join()
        .map_err(|_| io::Error::other("test thread panicked"))?
    }
}
