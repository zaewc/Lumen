//! The macOS launchd inventory: launch agents and daemons (roadmap 6.7).
//!
//! Reads the property lists in the five standard directories with the system's
//! own parser, so Lumen interprets them as launchd does. Only regular files of at
//! most 1 MiB are read. A symlink is followed only to a file on the sealed system
//! volume (`/System`): macOS ships some of its own agents that way, as links into
//! `/System/Cryptexes`. A property list without a `Label` is not a job (launchd
//! refuses to load it; Google's updater leaves empty ones behind). Each item's
//! program is signature-checked like applications are, except for Lumen's
//! helper, which reads as root and needs no signatures
//! ([`LaunchdInventory::without_signatures`]).
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
    /// Whether each item's program is signature-checked.
    signatures: bool,
}

impl LaunchdInventory {
    /// The standard directories: `/System/Library/Launch{Agents,Daemons}`,
    /// `/Library/Launch{Agents,Daemons}` and `~/Library/LaunchAgents`. Agents run
    /// as the user, daemons as root.
    pub fn new() -> Self {
        Self::for_home(std::env::var_os("HOME").as_deref().map(Path::new))
    }

    /// The system's directories and the agents of the account whose home is
    /// `home`. Lumen's helper names the caller's home, looked up by user ID,
    /// rather than its own `HOME` (ADR-0063 rule 4).
    #[must_use]
    pub fn for_home(home: Option<&Path>) -> Self {
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
        if let Some(home) = home {
            locations.push(Location {
                dir: home.join("Library/LaunchAgents"),
                kind: ServiceKind::LaunchAgent,
                scope: ServiceScope::User,
            });
        }
        Self {
            locations,
            signatures: true,
        }
    }

    /// The same, without checking any program's signature. Lumen's helper
    /// runs as root and needs only labels and programs; checking a signature
    /// would have root's Security framework read whatever file a plist in the
    /// caller's home names.
    #[must_use]
    pub fn without_signatures(self) -> Self {
        Self {
            signatures: false,
            ..self
        }
    }

    /// One directory of agents, for tests.
    pub fn agents_in(dir: PathBuf) -> Self {
        Self {
            locations: vec![Location {
                dir,
                kind: ServiceKind::LaunchAgent,
                scope: ServiceScope::User,
            }],
            signatures: true,
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

/// What one plist turned out to be.
enum Read {
    /// A launchd job.
    Job(Service),
    /// A readable property list that launchd would not load: it has no
    /// `Label`.
    NotAJob,
    /// It could not be read or understood: unknown, never absent.
    Unknown,
}

/// The bytes of the plist at `path`: the file itself, or the target of a
/// symlink that resolves under `/System` (the sealed, read-only system
/// volume). The file is opened without following a link and without waiting
/// (`O_NOFOLLOW | O_NONBLOCK`), then checked by its handle, so only a regular
/// file of at most [`MAX_PLIST_BYTES`] is read, even if the name is swapped for
/// a FIFO or a device after it was looked at.
fn read_plist(path: &Path) -> Option<Vec<u8>> {
    use std::io::Read as _;
    use std::os::unix::fs::OpenOptionsExt as _;

    let meta = std::fs::symlink_metadata(path).ok()?;
    let file = if meta.file_type().is_symlink() {
        let target = std::fs::canonicalize(path).ok()?;
        target.starts_with("/System/").then_some(target)?
    } else {
        path.to_path_buf()
    };
    let opened = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(file)
        .ok()?;
    let meta = opened.metadata().ok()?;
    if !meta.is_file() || meta.len() > MAX_PLIST_BYTES {
        return None;
    }
    let mut bytes = Vec::new();
    opened.take(MAX_PLIST_BYTES).read_to_end(&mut bytes).ok()?;
    Some(bytes)
}

/// Reads one plist into a service, checking its program's signature if
/// `signatures`.
fn service(path: &Path, location: &Location, signatures: bool) -> Read {
    let Some(list) = read_plist(path).and_then(|b| cf::property_list(&b)) else {
        return Read::Unknown;
    };
    let Some(job) = list.downcast_ref::<CFDictionary>() else {
        return Read::Unknown;
    };
    let Some(label) = cf::get::<CFString>(job, &CFString::from_str("Label")) else {
        return Read::NotAJob;
    };
    let label = label.to_string();
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
        .filter(|_| signatures)
        .and_then(CFURL::from_file_path)
        .map(|url| crate::apps::verify_signature(&url));
    Read::Job(Service {
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
                match service(&path, location, self.signatures) {
                    Read::Job(found) => services.push(found),
                    Read::NotAJob => {}
                    // An unreadable or malformed job is unknown, not absent.
                    Read::Unknown => complete = false,
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
    fn a_named_home_is_read_instead_of_the_environments() -> Result<(), Box<dyn std::error::Error>>
    {
        let fx = Fixture::build(&[file(
            "home/Library/LaunchAgents/com.example.mine.plist",
            plist(
                "<key>Label</key><string>com.example.mine</string>\
                 <key>Program</key><string>/bin/ls</string>",
            ),
        )])?;
        let mine = |inventory: &LaunchdInventory| {
            inventory
                .collect(&CancelToken::new())
                .services
                .iter()
                .any(|s| s.label.raw() == "com.example.mine")
        };
        let home = fx.path("home")?;
        let named = LaunchdInventory::for_home(Some(&home));
        assert!(mine(&named));
        assert_eq!(named.locations.len(), 5);
        assert_eq!(LaunchdInventory::for_home(None).locations.len(), 4);
        let dir = fx.path("home/Library/LaunchAgents")?;
        let agents = || LaunchdInventory::agents_in(dir.clone());
        let checked = agents().collect(&CancelToken::new());
        assert!(checked.services.iter().all(|s| s.signature.is_some()));
        let bare = agents().without_signatures().collect(&CancelToken::new());
        assert_eq!(bare.services.len(), 1);
        assert!(bare.services.iter().all(|s| s.signature.is_none()));
        Ok(())
    }

    #[test]
    fn a_list_without_a_label_is_not_a_job_and_a_link_into_the_system_is_read()
    -> Result<(), Box<dyn std::error::Error>> {
        // A job macOS itself ships, as the target of a link.
        let system = std::fs::read_dir("/System/Library/LaunchDaemons")?
            .filter_map(Result::ok)
            .map(|e| e.path())
            .find(|p| {
                p.extension().is_some_and(|e| e == "plist")
                    && std::fs::symlink_metadata(p).is_ok_and(|m| m.is_file())
                    && matches!(
                        service(p, &LaunchdInventory::new().locations[1], true),
                        Read::Job(_)
                    )
            })
            .ok_or("macOS ships launch daemons")?;
        let fx = Fixture::build(&[
            file("agents/empty.plist", plist("")),
            file("agents/settings.plist", plist("<key>Version4</key><dict/>")),
        ])?;
        std::os::unix::fs::symlink(&system, fx.path("agents")?.join("system.plist"))?;
        let report = LaunchdInventory::agents_in(fx.path("agents")?).collect(&CancelToken::new());
        assert_eq!(report.status, SourceStatus::Complete);
        assert_eq!(report.services.len(), 1, "only the linked system job");
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
        // A FIFO is not read, and does not hold the reader.
        let fx = Fixture::build(&[Node::Dir {
            path: "agents".into(),
        }])?;
        let status = std::process::Command::new("/usr/bin/mkfifo")
            .arg(fx.path("agents")?.join("fifo.plist"))
            .status()?;
        assert!(status.success());
        let report = LaunchdInventory::agents_in(fx.path("agents")?).collect(&CancelToken::new());
        assert_eq!(report.status, SourceStatus::Partial);
        Ok(())
    }
}
