//! Two signals that keep an app where it is (ADR-0060 rule 4, ADR-0063 rule
//! 1, owner decision 2026-11-16): security software, recognised by the
//! Endpoint Security entitlement in its signature, and apps an organization
//! may manage, recognised by the Mac's MDM enrollment or by managed
//! preferences for the app.
//!
//! Both ask the system's own tools, read-only, with no shell, closed stdin, a
//! clean environment, a time limit and a bounded output:
//!
//! - `/usr/bin/codesign -d --entitlements - --xml <bundle>` prints the main
//!   executable's signed entitlements as a property list (`codesign(1)`). It
//!   reads the signature in the executable; no other file content is read.
//!   Lumen's helper runs it as `nobody` ([`Run::AsNobody`]), so root never
//!   parses a binary another account could have written.
//! - `/usr/bin/profiles status -type enrollment` says whether the Mac is
//!   enrolled in MDM (`profiles(1)`). macOS keeps no documented, local record
//!   of which apps an MDM server installed or manages, so on an enrolled Mac
//!   every app counts as possibly managed.
//!
//! Anything unexpected (a tool missing, a timeout, unreadable output) is
//! unknown, which the gate treats as a refusal.

use std::ffi::OsStr;
use std::io::Read as _;
use std::os::unix::process::CommandExt as _;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use objc2_core_foundation::{CFArray, CFBoolean, CFDictionary, CFNumber, CFString};

/// The entitlement an Endpoint Security client must carry (Apple, "Endpoint
/// Security", `com.apple.developer.endpoint-security.client`).
pub const ENDPOINT_SECURITY: &str = "com.apple.developer.endpoint-security.client";

/// How long a tool may run.
const LIMIT: Duration = Duration::from_secs(10);

/// The most output read from a tool.
const MAX_OUTPUT: u64 = 1024 * 1024;

/// `nobody`'s user and group ID on macOS (-2).
const NOBODY: u32 = 0xFFFF_FFFE;

/// What `codesign` says on standard error for code with no signature.
const UNSIGNED: &[u8] = b"code object is not signed at all";

/// Who runs a tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Run {
    /// Lumen's own account.
    AsCaller,
    /// `nobody`, dropping root's rights first: for Lumen's helper.
    AsNobody,
}

/// Reads up to one byte past [`MAX_OUTPUT`] from `pipe` on its own thread,
/// so a tool that fills one pipe cannot block on it.
fn drain<R: std::io::Read + Send + 'static>(
    pipe: Option<R>,
) -> std::thread::JoinHandle<(bool, Vec<u8>)> {
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let read =
            pipe.is_some_and(|pipe| pipe.take(MAX_OUTPUT + 1).read_to_end(&mut bytes).is_ok());
        (read, bytes)
    })
}

/// A tool's exit status, standard output and standard error, or `None` if it
/// could not run, ran past [`LIMIT`] or wrote more than [`MAX_OUTPUT`].
fn run(program: &str, args: &[&OsStr], who: Run) -> Option<(bool, Vec<u8>, Vec<u8>)> {
    let mut command = Command::new(program);
    command
        .args(args)
        .env_clear()
        .env("PATH", "/usr/bin:/bin:/usr/sbin:/sbin")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if who == Run::AsNobody {
        // std drops the supplementary groups, then sets the group and user,
        // before the program starts.
        command.uid(NOBODY).gid(NOBODY);
    }
    let mut child = command.spawn().ok()?;
    let out = drain(child.stdout.take());
    let err = drain(child.stderr.take());
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if started.elapsed() < LIMIT => {
                std::thread::sleep(Duration::from_millis(20));
            }
            Ok(None) | Err(_) => {
                // Killing a child that already exited fails harmlessly.
                let _killed = child.kill();
                let _reaped = child.wait();
                break None;
            }
        }
    };
    let (out_ok, stdout) = out.join().ok()?;
    let (err_ok, stderr) = err.join().ok()?;
    let fits = |b: &Vec<u8>| u64::try_from(b.len()).is_ok_and(|n| n <= MAX_OUTPUT);
    if !(out_ok && err_ok && fits(&stdout) && fits(&stderr)) {
        return None;
    }
    Some((status?.success(), stdout, stderr))
}

/// Whether the entitlements `codesign` printed grant [`ENDPOINT_SECURITY`].
fn grants_endpoint_security(plist: &[u8]) -> Option<bool> {
    if plist.iter().all(u8::is_ascii_whitespace) {
        // A signature without entitlements.
        return Some(false);
    }
    let list = crate::cf::property_list(plist)?;
    let entitlements = list.downcast_ref::<CFDictionary>()?;
    let key = CFString::from_str(ENDPOINT_SECURITY);
    // Any value counts as a grant: only `false` or nothing is not.
    if let Some(value) = crate::cf::get::<CFBoolean>(entitlements, &key) {
        return Some(value.as_bool());
    }
    let other = crate::cf::get::<CFString>(entitlements, &key).is_some()
        || crate::cf::get::<CFArray>(entitlements, &key).is_some()
        || crate::cf::get::<CFDictionary>(entitlements, &key).is_some()
        || crate::cf::get::<CFNumber>(entitlements, &key).is_some();
    Some(other)
}

/// Whether the main executable of the bundle at `bundle` is signed with the
/// Endpoint Security entitlement; `Some(false)` for code with no signature,
/// which cannot hold one; `None` if that cannot be told.
#[must_use]
pub fn endpoint_security_client(bundle: &Path, who: Run) -> Option<bool> {
    let args = [
        OsStr::new("-d"),
        OsStr::new("--entitlements"),
        OsStr::new("-"),
        OsStr::new("--xml"),
        bundle.as_os_str(),
    ];
    let (ok, stdout, stderr) = run("/usr/bin/codesign", &args, who)?;
    if ok {
        return grants_endpoint_security(&stdout);
    }
    let unsigned = stderr.windows(UNSIGNED.len()).any(|w| w == UNSIGNED);
    unsigned.then_some(false)
}

/// The enrollment `profiles status -type enrollment` printed: whether its
/// `MDM enrollment:` line says `Yes` (with or without "User Approved") or
/// `No`, or `None` without exactly one such line.
fn enrollment(output: &[u8]) -> Option<bool> {
    let text = std::str::from_utf8(output).ok()?;
    let mut answers = text
        .lines()
        .filter_map(|line| line.trim().strip_prefix("MDM enrollment:"))
        .map(str::trim);
    let answer = answers.next()?;
    if answers.next().is_some() {
        return None;
    }
    match answer.split_whitespace().next()? {
        "Yes" => Some(true),
        "No" => Some(false),
        _ => None,
    }
}

/// Whether this Mac is enrolled in MDM; `None` if that cannot be told.
#[must_use]
pub fn mdm_enrolled() -> Option<bool> {
    let args = [
        OsStr::new("status"),
        OsStr::new("-type"),
        OsStr::new("enrollment"),
    ];
    let (ok, stdout, _) = run("/usr/bin/profiles", &args, Run::AsCaller)?;
    ok.then(|| enrollment(&stdout)).flatten()
}

/// Whether an organization may manage the app with bundle identifier `id`:
/// the Mac is enrolled in MDM, or managed preferences exist for the app,
/// machine-wide or for any account (`/Library/Managed Preferences`, where
/// macOS keeps preferences a profile enforces). `None` if that cannot be
/// told. Only names are looked at; no file is read.
#[must_use]
pub fn managed(id: &str) -> Option<bool> {
    managed_in(
        Path::new("/Library/Managed Preferences"),
        id,
        mdm_enrolled(),
    )
}

fn managed_in(root: &Path, id: &str, enrolled: Option<bool>) -> Option<bool> {
    if enrolled? {
        return Some(true);
    }
    if id.is_empty() || id.contains('/') || id.starts_with('.') {
        return None;
    }
    let name = format!("{id}.plist");
    let present = |dir: &Path| match std::fs::symlink_metadata(dir.join(&name)) {
        Ok(_) => Some(true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Some(false),
        Err(_) => None,
    };
    let entries = match std::fs::read_dir(root) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Some(false),
        Err(_) => return None,
    };
    let mut found = present(root)?;
    for entry in entries {
        let entry = entry.ok()?;
        // Per-account folders, never followed through a link.
        if entry.file_type().ok()?.is_dir() {
            found |= present(&entry.path())?;
        }
    }
    Some(found)
}

#[cfg(test)]
mod tests {
    use lumen_testkit::{Fixture, Node};

    use super::*;

    fn plist(body: &str) -> Vec<u8> {
        format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<plist version=\"1.0\"><dict>{body}</dict></plist>"
        )
        .into_bytes()
    }

    #[test]
    fn the_entitlement_is_read_from_codesigns_property_list() {
        let es = format!("<key>{ENDPOINT_SECURITY}</key>");
        assert_eq!(
            grants_endpoint_security(&plist(&format!("{es}<true/>"))),
            Some(true)
        );
        assert_eq!(
            grants_endpoint_security(&plist(&format!("{es}<false/>"))),
            Some(false)
        );
        assert_eq!(
            grants_endpoint_security(&plist(&format!("{es}<string>x</string>"))),
            Some(true)
        );
        assert_eq!(
            grants_endpoint_security(&plist("<key>com.apple.security.app-sandbox</key><true/>")),
            Some(false)
        );
        assert_eq!(grants_endpoint_security(b""), Some(false));
        assert_eq!(grants_endpoint_security(b"\n"), Some(false));
        assert_eq!(grants_endpoint_security(b"<plist><dict><key>"), None);
        assert_eq!(
            grants_endpoint_security(b"<plist version=\"1.0\"><array/></plist>"),
            None
        );
    }

    #[test]
    fn codesign_answers_for_signed_unsigned_and_missing_code() {
        // Apple's own app, signed, without the entitlement.
        let calculator = Path::new("/System/Applications/Calculator.app");
        assert_eq!(
            endpoint_security_client(calculator, Run::AsCaller),
            Some(false)
        );
        assert_eq!(
            endpoint_security_client(Path::new("/nonexistent/X.app"), Run::AsCaller),
            None
        );
        // Dropping to `nobody` needs root; without it the tool never starts,
        // and the answer is unknown rather than "no".
        if std::fs::metadata(std::env::temp_dir())
            .is_ok_and(|m| std::os::unix::fs::MetadataExt::uid(&m) != 0)
        {
            assert_eq!(endpoint_security_client(calculator, Run::AsNobody), None);
        }
    }

    #[test]
    fn enrollment_is_read_from_its_one_line() {
        let yes = b"Enrolled via DEP: Yes\nMDM enrollment: Yes (User Approved)\nMDM server: x\n";
        assert_eq!(enrollment(yes), Some(true));
        assert_eq!(
            enrollment(b"Enrolled via DEP: No\nMDM enrollment: No\n"),
            Some(false)
        );
        assert_eq!(enrollment(b"MDM enrollment: Yes\n"), Some(true));
        for unknown in [
            &b""[..],
            b"Enrolled via DEP: No\n",
            b"MDM enrollment: maybe\n",
            b"MDM enrollment: No\nMDM enrollment: Yes\n",
            b"\xff\xfe",
        ] {
            assert_eq!(enrollment(unknown), None, "{unknown:?}");
        }
        // This Mac answers one way or the other.
        assert!(mdm_enrolled().is_some());
    }

    #[test]
    fn managed_preferences_for_the_app_mark_it_managed() -> Result<(), Box<dyn std::error::Error>> {
        let fx = Fixture::build(&[
            Node::File {
                path: "prefs/com.example.word.plist".into(),
                contents: b"x".to_vec(),
            },
            Node::File {
                path: "prefs/ana/com.example.excel.plist".into(),
                contents: b"x".to_vec(),
            },
            Node::Dir {
                path: "prefs/ben".into(),
            },
        ])?;
        let root = fx.path("prefs")?;
        assert_eq!(
            managed_in(&root, "com.example.word", Some(false)),
            Some(true)
        );
        assert_eq!(
            managed_in(&root, "com.example.excel", Some(false)),
            Some(true)
        );
        assert_eq!(
            managed_in(&root, "com.example.notes", Some(false)),
            Some(false)
        );
        // An enrolled Mac may manage any app; unknown enrollment is unknown.
        assert_eq!(
            managed_in(&root, "com.example.notes", Some(true)),
            Some(true)
        );
        assert_eq!(managed_in(&root, "com.example.notes", None), None);
        assert_eq!(managed_in(&root, "../x", Some(false)), None);
        assert_eq!(managed_in(&root, "", Some(false)), None);
        assert_eq!(
            managed_in(&root.join("missing"), "com.example.word", Some(false)),
            Some(false)
        );
        Ok(())
    }
}
