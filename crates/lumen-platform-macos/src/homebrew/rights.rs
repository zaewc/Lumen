//! Whether Homebrew's own uninstall of a formula or cask runs with the
//! person's rights (ADR-0060 rule 6, roadmap 16.7), from Homebrew's records
//! and the folders it would change. `brew` is never run.
//!
//! - A formula: its keg folder `Cellar/<name>` is the person's, in a folder
//!   they may write.
//! - A cask: every step its `uninstall_artifacts` (or stored definition)
//!   lists is one Homebrew takes with the person's rights, and every app it
//!   placed is theirs, in a folder they may write. `pkg`, `installer`,
//!   `uninstall pkgutil:` and `kext` always need administrator rights; a
//!   script, `launchctl`, `delete`, `trash` or `rmdir` may, so it is unknown;
//!   a step Lumen does not know is unknown. `zap` is never run (no `--zap`).
//!
//! `Some(false)` means it needs administrator rights, `None` that it cannot
//! be told: both keep the package where it is.

#![allow(unsafe_code)]

use std::ffi::CString;
use std::os::unix::ffi::OsStrExt as _;
use std::os::unix::fs::MetadataExt as _;
use std::path::Path;

use serde_json::Value;

/// How one listed step is taken.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Step {
    /// With the person's rights (an app also needs [`mine`]).
    Person,
    /// It may or may not need administrator rights.
    Unknown,
    /// It always needs administrator rights.
    Administrator,
}

/// One artifact of a cask's list.
fn artifact(key: &str, value: &Value) -> Step {
    match key {
        "app" | "binary" | "manpage" | "bash_completion" | "zsh_completion" | "fish_completion"
        | "zap" => Step::Person,
        "pkg" | "installer" => Step::Administrator,
        "uninstall" => value.as_array().map_or(Step::Unknown, |directives| {
            directives
                .iter()
                .flat_map(|d| d.as_object().into_iter().flatten())
                .map(|(directive, _)| uninstall(directive))
                .max()
                .unwrap_or(Step::Person)
        }),
        _ => Step::Unknown,
    }
}

/// One directive of a cask's `uninstall` stanza.
fn uninstall(directive: &str) -> Step {
    match directive {
        "quit" | "signal" | "login_item" => Step::Person,
        "pkgutil" | "kext" => Step::Administrator,
        _ => Step::Unknown,
    }
}

/// How a cask's listed steps are taken, all together.
pub(super) fn cask_steps(artifacts: &[Value]) -> Option<bool> {
    let worst = artifacts
        .iter()
        .flat_map(|a| a.as_object().into_iter().flatten())
        .map(|(key, value)| artifact(key, value))
        .max()
        .unwrap_or(Step::Person);
    match worst {
        Step::Person => Some(true),
        Step::Unknown => None,
        Step::Administrator => Some(false),
    }
}

/// Whether this process may write in `dir`, as `access(2)` answers for the
/// real user; `None` if that cannot be asked.
fn writable(dir: &Path) -> Option<bool> {
    let path = CString::new(dir.as_os_str().as_bytes()).ok()?;
    // SAFETY: invariant: `path` is a NUL-terminated string that lives for the
    // call. Why unsafe: FFI. Why it holds: it comes from `CString::new`, and
    // `access` only reads it.
    let status = unsafe { libc::access(path.as_ptr(), libc::W_OK) };
    Some(status == 0)
}

/// This process's real user ID.
fn me() -> u32 {
    // SAFETY: invariant: none. Why unsafe: FFI. Why it holds: `getuid` takes
    // no arguments, cannot fail and touches no memory.
    unsafe { libc::getuid() }
}

/// Whether `path` (not followed) belongs to this process's user, in a folder
/// it may write; `None` if either cannot be read.
pub(super) fn mine(path: &Path) -> Option<bool> {
    let meta = std::fs::symlink_metadata(path).ok()?;
    let parent = path.parent()?;
    Some(meta.uid() == me() && writable(parent)?)
}

#[cfg(test)]
mod tests {
    use lumen_testkit::{Fixture, Node};
    use serde_json::json;

    use super::*;

    #[test]
    fn apps_and_links_alone_run_with_the_persons_rights() {
        let artifacts = [
            json!({"app": ["Hot.app"]}),
            json!({"binary": ["/opt/homebrew/Caskroom/x/1/x"]}),
            json!({"uninstall": [{"quit": "com.x"}, {"signal": ["TERM", "com.x"]}]}),
            json!({"zap": [{"trash": ["~/Library/Caches/com.x"]}]}),
        ];
        assert_eq!(cask_steps(&artifacts), Some(true));
        assert_eq!(cask_steps(&[]), Some(true));
    }

    #[test]
    fn installer_packages_and_kernel_extensions_need_an_administrator() {
        for artifacts in [
            vec![json!({"uninstall": [{"pkgutil": "com.microsoft.powershell"}]})],
            vec![json!({"pkg": ["x.pkg"]}), json!({"app": ["X.app"]})],
            vec![json!({"uninstall": [{"kext": "com.x.driver"}, {"script": {}}]})],
        ] {
            assert_eq!(cask_steps(&artifacts), Some(false), "{artifacts:?}");
        }
    }

    #[test]
    fn a_script_a_daemon_a_deletion_or_anything_unknown_cannot_be_told() {
        for artifacts in [
            vec![json!({"uninstall": [{"script": {"executable": "x", "sudo": true}}]})],
            vec![json!({"uninstall": [{"launchctl": "com.x.helper"}]})],
            vec![json!({"uninstall": [{"delete": "/Library/X"}]})],
            vec![json!({"uninstall": "not a list"})],
            vec![json!({"something_new": ["x"]})],
        ] {
            assert_eq!(cask_steps(&artifacts), None, "{artifacts:?}");
        }
    }

    #[test]
    fn my_own_folder_is_mine_and_roots_is_not() {
        let fx = Fixture::build(&[Node::Dir {
            path: "Applications/X.app".into(),
        }])
        .unwrap_or_else(|e| unreachable!("{e}"));
        let app = fx.root().join("Applications/X.app");
        assert_eq!(mine(&app), Some(true));
        assert_eq!(mine(&fx.root().join("Applications/missing.app")), None);
        if me() != 0 {
            assert_eq!(mine(Path::new("/System")), Some(false));
        }
    }
}
