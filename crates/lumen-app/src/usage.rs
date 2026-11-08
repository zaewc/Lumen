//! Where Spotlight finds the applications that own candidates (ADR-0059): a
//! read-only query through the system's own `mdfind`, with fixed arguments, no
//! shell and the inspection time and size limits. When an application was
//! last opened is read in process (`lumen_platform_macos::metadata`). They run only for applications that own a candidate, never as an
//! inventory of everything the person uses, and nothing they return leaves
//! the device.

use std::path::{Path, PathBuf};

use crate::inspection::{Commands, InspectionError};

/// Where Spotlight finds an application with `bundle_id`, on every indexed
/// volume. Empty says only that Spotlight found none: unindexed and unmounted
/// volumes are not searched, so it never means "not installed".
///
/// # Errors
///
/// The identifier is not a plain bundle identifier, or the query failed.
pub fn found_installed(bundle_id: &str) -> Result<Vec<PathBuf>, InspectionError> {
    found_with(
        &mut crate::inspection::Program::system("/usr/bin/mdfind")?,
        bundle_id,
    )
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
