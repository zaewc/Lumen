//! Who an account is, looked up by its user ID, for Lumen's helper
//! (ADR-0063 rule 4).
//!
//! The helper learns the caller's user ID from the connection. Everything else
//! about that account (its home folder, whether it is an administrator) is
//! read here from the directory service by that ID, never from the helper's
//! own environment (`HOME`) or from the request.

#![allow(unsafe_code)]

use std::ffi::{CStr, CString, c_char, c_int};
use std::mem::MaybeUninit;
use std::os::unix::ffi::OsStrExt as _;
use std::path::PathBuf;

/// The `admin` group's ID on macOS: its members are the administrators.
const ADMIN_GID: c_int = 80;

/// The largest buffer tried for one account's record.
const MAX_RECORD: usize = 1 << 20;

/// The most groups read for one account.
const MAX_GROUPS: usize = 4096;

/// An account's name, primary group and home folder.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Account {
    name: CString,
    gid: u32,
    home: PathBuf,
}

/// The account with user ID `uid`, or `None` if there is none or it cannot be
/// read.
fn account(uid: u32) -> Option<Account> {
    let mut size = 4096;
    loop {
        let mut buffer: Vec<c_char> = vec![0; size];
        let mut record = MaybeUninit::<libc::passwd>::uninit();
        let mut found: *mut libc::passwd = std::ptr::null_mut();
        // SAFETY: invariant: `record` and `buffer` are valid for writes of a
        // `passwd` and `buffer.len()` bytes, and `found` for one pointer, for
        // the whole call. Why unsafe: FFI. Why it holds: all three are locals
        // that outlive the call, and the length passed is the buffer's own.
        let status = unsafe {
            libc::getpwuid_r(
                uid,
                record.as_mut_ptr(),
                buffer.as_mut_ptr(),
                buffer.len(),
                &raw mut found,
            )
        };
        if status == libc::ERANGE && size < MAX_RECORD {
            size *= 2;
            continue;
        }
        if status != 0 || found.is_null() {
            return None;
        }
        // SAFETY: invariant: on success `getpwuid_r` initialized `record`,
        // whose strings point into `buffer`, NUL-terminated. Why unsafe:
        // reading memory written by C. Why it holds: the call returned 0 with
        // `found` pointing at `record`, and `buffer` is still alive; the
        // strings are copied before it is dropped.
        let (name, home, gid) = unsafe {
            let record = record.assume_init_ref();
            if record.pw_name.is_null() || record.pw_dir.is_null() {
                return None;
            }
            (
                CStr::from_ptr(record.pw_name).to_owned(),
                CStr::from_ptr(record.pw_dir).to_bytes().to_vec(),
                record.pw_gid,
            )
        };
        return Some(Account {
            name,
            gid,
            home: PathBuf::from(std::ffi::OsStr::from_bytes(&home)),
        });
    }
}

/// The home folder of the account with user ID `uid`, as the directory
/// service records it; `None` if there is no such account or it has no
/// absolute home.
#[must_use]
pub fn home(uid: u32) -> Option<PathBuf> {
    account(uid)
        .map(|account| account.home)
        .filter(|home| home.is_absolute())
}

/// Whether the account with user ID `uid` is an administrator (a member of
/// the `admin` group, directly or through nested groups); `None` if that
/// cannot be read.
#[must_use]
pub fn administrator(uid: u32) -> Option<bool> {
    let account = account(uid)?;
    let base = c_int::try_from(account.gid).ok()?;
    let mut groups: Vec<c_int> = vec![0; MAX_GROUPS];
    let mut count = c_int::try_from(groups.len()).ok()?;
    // SAFETY: invariant: `name` is NUL-terminated, `groups` is valid for
    // writes of `count` entries and `count` for one `int`. Why unsafe: FFI.
    // Why it holds: `name` is a `CString` and `count` starts at the vector's
    // own length; both outlive the call.
    let status = unsafe {
        libc::getgrouplist(
            account.name.as_ptr(),
            base,
            groups.as_mut_ptr(),
            &raw mut count,
        )
    };
    if status != 0 {
        // The list did not fit, so membership is not known.
        return None;
    }
    let count = usize::try_from(count).ok()?.min(groups.len());
    Some(groups.get(..count)?.contains(&ADMIN_GID))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn me() -> u32 {
        // SAFETY: invariant: none. Why unsafe: FFI. Why it holds: `getuid`
        // takes no arguments, cannot fail and touches no memory.
        unsafe { libc::getuid() }
    }

    #[test]
    fn root_is_an_administrator_with_its_own_home() {
        assert_eq!(administrator(0), Some(true));
        assert_eq!(home(0), Some(PathBuf::from("/var/root")));
    }

    #[test]
    fn the_running_account_is_found_by_its_id() {
        let found = account(me());
        assert!(found.is_some());
        assert!(administrator(me()).is_some());
        assert!(home(me()).is_some_and(|home| home.is_absolute()));
    }

    #[test]
    fn an_account_that_does_not_exist_is_unknown() {
        let nobody_has = 0x7fff_fff0;
        assert_eq!(home(nobody_has), None);
        assert_eq!(administrator(nobody_has), None);
    }
}
