//! What macOS records about how the applications that own candidates are used
//! (ADR-0059): read-only Spotlight queries through the system's own `mdls` and
//! `mdfind`, with fixed arguments, no shell and the inspection time and size
//! limits. They run only for applications that own a candidate, never as an
//! inventory of everything the person uses, and nothing they return leaves
//! the device.

use std::path::{Path, PathBuf};

use lumen_domain::Timestamp;

use crate::inspection::{Commands, InspectionError};

/// When `LaunchServices` last recorded the application bundle at `bundle` being
/// opened, `None` if it never recorded one. Launches outside `LaunchServices`
/// (command line, other programs) are not recorded, and Apple reports the date
/// sometimes fails to update: never proof the application is unused.
///
/// # Errors
///
/// The query failed, timed out, or printed something unexpected.
#[cfg(target_os = "macos")]
pub fn last_opened(bundle: &Path) -> Result<Option<Timestamp>, InspectionError> {
    last_opened_with(
        &mut crate::inspection::Program::system("/usr/bin/mdls")?,
        bundle,
    )
}

/// Where Spotlight finds an application with `bundle_id`, on every indexed
/// volume. Empty says only that Spotlight found none: unindexed and unmounted
/// volumes are not searched, so it never means "not installed".
///
/// # Errors
///
/// The identifier is not a plain bundle identifier, or the query failed.
#[cfg(target_os = "macos")]
pub fn found_installed(bundle_id: &str) -> Result<Vec<PathBuf>, InspectionError> {
    found_with(
        &mut crate::inspection::Program::system("/usr/bin/mdfind")?,
        bundle_id,
    )
}

fn last_opened_with(
    commands: &mut impl Commands,
    bundle: &Path,
) -> Result<Option<Timestamp>, InspectionError> {
    let path = bundle
        .to_str()
        .ok_or(InspectionError::Format("bundle path"))?;
    let out = commands.query(&["-raw", "-name", "kMDItemLastUsedDate", "--", path])?;
    let value = out.trim();
    if value == "(null)" {
        return Ok(None);
    }
    // `mdls` prints UTC as `2026-10-03 09:12:44 +0000`.
    let utc = value
        .strip_suffix(" +0000")
        .filter(|day_time| day_time.len() == 19 && day_time.as_bytes()[10] == b' ')
        .ok_or(InspectionError::Format("last-opened date"))?;
    let rfc3339 = format!("{}T{}Z", &utc[..10], &utc[11..]);
    rfc3339
        .parse()
        .map(Some)
        .map_err(|_| InspectionError::Format("last-opened date"))
}

/// A bundle identifier as Apple allows it (letters, digits, `.` and `-`), so
/// it can stand in a Spotlight query without changing the query.
fn plain_bundle_id(id: &str) -> bool {
    (1..=255).contains(&id.len())
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-')
}

fn found_with(
    commands: &mut impl Commands,
    bundle_id: &str,
) -> Result<Vec<PathBuf>, InspectionError> {
    if !plain_bundle_id(bundle_id) {
        return Err(InspectionError::Format("bundle identifier"));
    }
    let query = format!("kMDItemCFBundleIdentifier == \"{bundle_id}\"");
    let out = commands.query(&["-0", &query])?;
    Ok(out
        .split('\0')
        .filter(|path| {
            path.starts_with('/')
                && Path::new(path)
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("app"))
        })
        .map(PathBuf::from)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Answers one query, recording the arguments it was asked with.
    struct Fake {
        answer: Result<String, &'static str>,
        asked: Vec<Vec<String>>,
    }

    impl Commands for Fake {
        fn query(&mut self, args: &[&str]) -> Result<String, InspectionError> {
            self.asked
                .push(args.iter().map(ToString::to_string).collect());
            self.answer.clone().map_err(InspectionError::Format)
        }
    }

    fn fake(answer: Result<&str, &'static str>) -> Fake {
        Fake {
            answer: answer.map(str::to_owned),
            asked: Vec::new(),
        }
    }

    #[test]
    fn a_recorded_launch_is_a_time_and_none_is_not_a_date() -> Result<(), Box<dyn std::error::Error>>
    {
        let bundle = Path::new("/Applications/Chat.app");
        let mut recorded = fake(Ok("2026-10-03 09:12:44 +0000\n"));
        assert_eq!(
            last_opened_with(&mut recorded, bundle)?,
            Some("2026-10-03T09:12:44Z".parse()?)
        );
        assert_eq!(
            recorded.asked,
            [[
                "-raw",
                "-name",
                "kMDItemLastUsedDate",
                "--",
                "/Applications/Chat.app"
            ]]
        );
        assert_eq!(last_opened_with(&mut fake(Ok("(null)")), bundle)?, None);
        for odd in ["yesterday", "2026-10-03 09:12:44 +0900", ""] {
            assert!(
                last_opened_with(&mut fake(Ok(odd)), bundle).is_err(),
                "{odd:?} is never guessed at"
            );
        }
        assert!(last_opened_with(&mut fake(Err("denied")), bundle).is_err());
        Ok(())
    }

    #[test]
    fn only_plain_bundle_ids_are_looked_up_and_only_apps_count() -> Result<(), InspectionError> {
        let mut spotlight = fake(Ok(
            "/Volumes/Work/Applications/Chat.app\0/Users/a/Chat.app/x\0\0",
        ));
        assert_eq!(
            found_with(&mut spotlight, "com.example.Chat")?,
            [PathBuf::from("/Volumes/Work/Applications/Chat.app")]
        );
        assert_eq!(
            spotlight.asked,
            [["-0", "kMDItemCFBundleIdentifier == \"com.example.Chat\""]]
        );
        for hostile in [
            "",
            "com.x\" || kMDItemFSName == \"*",
            "com x",
            "../x",
            "com.x\n",
        ] {
            let mut never = fake(Ok("/Applications/X.app"));
            assert!(found_with(&mut never, hostile).is_err(), "{hostile:?}");
            assert!(never.asked.is_empty(), "nothing runs for {hostile:?}");
        }
        assert_eq!(
            found_with(&mut fake(Ok("")), "com.example.Chat")?,
            Vec::<PathBuf>::new()
        );
        Ok(())
    }
}
