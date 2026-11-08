//! What Spotlight records about an application bundle (ADR-0059), read in
//! process through `MDItem`, never by running a program.

#![allow(unsafe_code)]

use std::os::unix::ffi::OsStrExt as _;
use std::path::Path;

use lumen_domain::Timestamp;
use objc2_core_foundation::{CFDate, CFString};
use objc2_core_services::{MDItem, kMDItemLastUsedDate};

/// Seconds from 1970-01-01 to Core Foundation's reference date, 2001-01-01.
const CF_EPOCH: f64 = 978_307_200.0;

/// When `LaunchServices` last recorded the bundle at `bundle` being opened,
/// `None` when it recorded none or Spotlight has no item for it. Launches
/// outside `LaunchServices` (command line, other programs) are not recorded,
/// and Apple reports the date sometimes fails to update: never proof the
/// application is unused.
#[must_use]
pub fn last_opened(bundle: &Path) -> Option<Timestamp> {
    let path = CFString::from_str(std::str::from_utf8(bundle.as_os_str().as_bytes()).ok()?);
    // SAFETY: invariant: both arguments are valid or `None` where allowed.
    // Why unsafe: FFI call. Why it holds: the default allocator (`None`) is
    // documented as allowed, and `path` is a live CFString.
    let item = unsafe { MDItem::new(None, Some(&path)) }?;
    // SAFETY: invariant: the extern static is a constant CFString provided by
    // CoreServices. Why unsafe: reading an extern static. Why it holds: it is
    // initialised by the framework before any code runs and never written.
    let key = unsafe { kMDItemLastUsedDate }?;
    // SAFETY: invariant: `item` is a live MDItem and `key` an attribute name.
    // Why unsafe: FFI call. Why it holds: both come from CoreServices above.
    let value = unsafe { item.attribute(Some(key)) }?;
    let date = value.downcast::<CFDate>().ok()?;
    let seconds = date.absolute_time() + CF_EPOCH;
    if !seconds.is_finite() {
        return None;
    }
    let whole = seconds.floor();
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a finite date within Timestamp's range, and a fraction in [0, 1)"
    )]
    let (secs, nanos) = (whole as i64, ((seconds - whole) * 1e9) as u32);
    Timestamp::from_unix(secs, nanos.min(999_999_999)).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_bundle_has_no_date_and_a_date_is_never_in_the_future() {
        assert_eq!(last_opened(Path::new("/nonexistent/Nothing.app")), None);
        // Whatever this Mac recorded for a system app, it is not later than now.
        if let Some(at) = last_opened(Path::new("/System/Applications/Calculator.app")) {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs());
            assert!(at.unix_seconds() <= i64::try_from(now).unwrap_or(i64::MAX));
        }
    }
}
