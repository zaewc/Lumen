//! The macOS launchd inventory: launch agents and daemons (roadmap 6.7).
//!
//! Reads the property lists in the five standard directories with the system's
//! own parser, so Lumen interprets them as launchd does. Only regular files of at
//! most 1 MiB are read, and never through symlinks. Each item's program is
//! signature-checked like applications are.
//!
//! `enabled` is left unknown: a plist's `Disabled` key is only a default that
//! launchd's override database (`launchctl enable`/`disable`) can change.

use std::os::unix::ffi::OsStrExt as _;
use std::path::{Path, PathBuf};

use lumen_application::CancelToken;
use lumen_application::ports::{InventoryReport, InventorySource};
use lumen_domain::{
    RawPath, Service, ServiceKind, ServiceScope, SourceName, SourceStatus, UntrustedText,
};
use objc2_core_foundation::{CFArray, CFDictionary, CFString, CFURL};

use crate::cf;

/// The inventory source name.
const SOURCE: &str = "macos.launchd-plists";
/// Largest plist read; real ones are a few kilobytes.
const MAX_PLIST_BYTES: u64 = 1024 * 1024;

/// One directory of launchd plists and what its items are.
#[derive(Debug, Clone)]
struct Location {
    dir: PathBuf,
    kind: ServiceKind,
    scope: ServiceScope,
}

/// Lists launch agents and daemons.
#[derive(Debug, Clone)]
pub struct LaunchdInventory {
    locations: Vec<Location>,
}

impl LaunchdInventory {
    /// The standard directories: `/System/Library/Launch{Agents,Daemons}`,
    /// `/Library/Launch{Agents,Daemons}` and `~/Library/LaunchAgents`. Agents run
    /// as the user, daemons as root.
    pub fn new() -> Self {
        let agent = |dir: &str| Location {
            dir: PathBuf::from(dir),
            kind: ServiceKind::LaunchAgent,
            scope: ServiceScope::User,
        };
        let daemon = |dir: &str| Location {
            dir: PathBuf::from(dir),
            kind: ServiceKind::LaunchDaemon,
            scope: ServiceScope::System,
        };
        let mut locations = vec![
            agent("/System/Library/LaunchAgents"),
            daemon("/System/Library/LaunchDaemons"),
            agent("/Library/LaunchAgents"),
            daemon("/Library/LaunchDaemons"),
        ];
        if let Some(home) = std::env::var_os("HOME") {
            locations.push(Location {
                dir: Path::new(&home).join("Library/LaunchAgents"),
                kind: ServiceKind::LaunchAgent,
                scope: ServiceScope::User,
            });
        }
        Self { locations }
    }

    /// One directory of agents, for tests.
    pub fn agents_in(dir: PathBuf) -> Self {
        Self {
            locations: vec![Location {
                dir,
                kind: ServiceKind::LaunchAgent,
                scope: ServiceScope::User,
            }],
        }
    }
}

impl Default for LaunchdInventory {
    fn default() -> Self {
        Self::new()
    }
}

fn source() -> SourceName {
    SourceName::new(SOURCE).unwrap_or_else(|_| unreachable!("valid source name"))
}

fn raw(path: &Path) -> Option<RawPath> {
    RawPath::from_unix_bytes(path.as_os_str().as_bytes().to_vec()).ok()
}

/// Reads one plist into a service. `None` if it is not a readable launchd job.
fn service(path: &Path, location: &Location) -> Option<Service> {
    // Regular files only: a symlink is not followed, and a FIFO or device would
    // block or never end.
    let meta = std::fs::symlink_metadata(path).ok()?;
    if !meta.is_file() || meta.len() > MAX_PLIST_BYTES {
        return None;
    }
    let bytes = std::fs::read(path).ok()?;
    let list = cf::property_list(&bytes)?;
    let job = list.downcast_ref::<CFDictionary>()?;
    let label = cf::get::<CFString>(job, &CFString::from_str("Label"))?.to_string();
    let program = cf::get::<CFString>(job, &CFString::from_str("Program"))
        .map(ToString::to_string)
        .or_else(|| {
            cf::get::<CFArray>(job, &CFString::from_str("ProgramArguments"))
                .and_then(|args| cf::element::<CFString>(args, 0))
                .map(ToString::to_string)
        })
        // launchd resolves bare names through a search path; only absolute
        // paths name a file Lumen can relate to.
        .filter(|p| p.starts_with('/'));
    let signature = program
        .as_deref()
        .and_then(CFURL::from_file_path)
        .map(|url| crate::apps::verify_signature(&url));
    Some(Service {
        kind: location.kind,
        scope: location.scope,
        label: UntrustedText::new(label),
        definition: raw(path),
        program: program.as_deref().map(Path::new).and_then(raw),
        enabled: None,
        signature,
    })
}

impl InventorySource for LaunchdInventory {
    fn name(&self) -> SourceName {
        source()
    }

    fn collect(&self, cancel: &CancelToken) -> InventoryReport {
        let mut services = Vec::new();
        let mut complete = true;
        'locations: for location in &self.locations {
            let entries = match std::fs::read_dir(&location.dir) {
                Ok(entries) => entries,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(_) => {
                    complete = false;
                    continue;
                }
            };
            for entry in entries {
                if cancel.is_cancelled() {
                    complete = false;
                    break 'locations;
                }
                let Ok(entry) = entry else {
                    complete = false;
                    continue;
                };
                let path = entry.path();
                if path.extension().is_none_or(|e| e != "plist") {
                    continue;
                }
                match service(&path, location) {
                    Some(found) => services.push(found),
                    // An unreadable or malformed job is unknown, not absent.
                    None => complete = false,
                }
            }
        }
        InventoryReport {
            source: source(),
            status: if complete {
                SourceStatus::Complete
            } else {
                SourceStatus::Partial
            },
            applications: Vec::new(),
            services,
            packages: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use lumen_domain::SignatureCheck;
    use lumen_testkit::{Fixture, Node};

    use super::*;

    fn plist(body: &str) -> Vec<u8> {
        format!("<?xml version=\"1.0\"?><plist version=\"1.0\"><dict>{body}</dict></plist>")
            .into_bytes()
    }

    fn file(path: &str, contents: Vec<u8>) -> Node {
        Node::File {
            path: path.into(),
            contents,
        }
    }

    #[test]
    fn finds_apple_daemons_with_verified_programs() {
        let report = LaunchdInventory::new().collect(&CancelToken::new());
        let daemons: Vec<_> = report
            .services
            .iter()
            .filter(|s| {
                s.kind == ServiceKind::LaunchDaemon && s.label.raw().starts_with("com.apple.")
            })
            .collect();
        assert!(daemons.len() > 10, "macOS ships many com.apple daemons");
        assert!(
            daemons.iter().any(|s| s
                .signature
                .as_ref()
                .is_some_and(|sig| sig.check == SignatureCheck::SignerVerified)),
            "some Apple daemon programs verify"
        );
    }

    #[test]
    fn reads_jobs_and_marks_unreadable_ones_partial() -> Result<(), Box<dyn std::error::Error>> {
        let fx = Fixture::build(&[
            file(
                "agents/com.example.args.plist",
                plist(
                    "<key>Label</key><string>com.example.args</string>\
                     <key>ProgramArguments</key><array><string>/usr/bin/true</string><string>-x</string></array>",
                ),
            ),
            file(
                "agents/com.example.program.plist",
                plist(
                    "<key>Label</key><string>com.example.program</string>\
                     <key>Program</key><string>/bin/ls</string><key>Disabled</key><true/>",
                ),
            ),
            file(
                "agents/com.example.bare.plist",
                plist(
                    "<key>Label</key><string>com.example.bare</string><key>Program</key><string>tool</string>",
                ),
            ),
            file("agents/notes.txt", b"ignored".to_vec()),
        ])?;
        let report = LaunchdInventory::agents_in(fx.path("agents")?).collect(&CancelToken::new());
        assert_eq!(report.status, SourceStatus::Complete);
        let mut found: Vec<_> = report
            .services
            .iter()
            .map(|s| {
                (
                    s.label.raw().to_owned(),
                    s.program.as_ref().map(RawPath::display),
                    s.enabled,
                )
            })
            .collect();
        found.sort();
        assert_eq!(
            found,
            [
                (
                    "com.example.args".to_owned(),
                    Some("/usr/bin/true".to_owned()),
                    None
                ),
                ("com.example.bare".to_owned(), None, None),
                (
                    "com.example.program".to_owned(),
                    Some("/bin/ls".to_owned()),
                    None
                ),
            ],
            "Disabled is only a default, so enabled stays unknown"
        );
        Ok(())
    }

    #[test]
    fn malformed_linked_and_oversized_plists_are_not_trusted()
    -> Result<(), Box<dyn std::error::Error>> {
        let cases: [Vec<Node>; 4] = [
            vec![file("agents/broken.plist", b"<plist><dict><key>".to_vec())],
            vec![file(
                "agents/array.plist",
                b"<plist version=\"1.0\"><array/></plist>".to_vec(),
            )],
            vec![
                file("real.plist", plist("<key>Label</key><string>x</string>")),
                Node::Symlink {
                    path: "agents/link.plist".into(),
                    target: "../real.plist".into(),
                },
            ],
            vec![file("agents/huge.plist", vec![b' '; 1024 * 1024 + 1])],
        ];
        for nodes in cases {
            let fx = Fixture::build(&nodes)?;
            let report =
                LaunchdInventory::agents_in(fx.path("agents")?).collect(&CancelToken::new());
            assert_eq!(report.status, SourceStatus::Partial, "{nodes:?}");
            assert_eq!(report.services.len(), 0);
        }
        Ok(())
    }
}
