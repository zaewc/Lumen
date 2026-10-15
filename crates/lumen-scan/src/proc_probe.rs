//! The Linux [`InUseProbe`] on `/proc` (roadmap 10.16, ADR-0030).
//!
//! For every process it follows the open file descriptors, the working
//! directory and the executable to the objects they name, and reads the
//! mapped files. A process uses the object when one of them is the object
//! itself (by device and inode) or, for a directory, lies under its canonical
//! path. The object must still be the scanned one (the enumerators' identity
//! rule), or the answer is unknown.
//!
//! An unprivileged process cannot read other users' descriptors: those
//! processes are counted as unseen and the answer is [`InUse::Partial`], never
//! a claim of non-use. A process that exits while being read is skipped.

use std::fs;
use std::io::ErrorKind;
use std::os::unix::ffi::OsStrExt as _;
use std::os::unix::fs::MetadataExt as _;
use std::path::{Path, PathBuf};

use lumen_application::ports::{InUse, InUseProbe};
use lumen_domain::{FileIdentity, PathFlavor, Process, RawPath, VolumeId};

/// Asks `/proc` which processes use an object.
#[derive(Debug, Clone, Copy, Default)]
pub struct ProcProbe;

/// What to look for.
struct Target {
    device: u64,
    inode: u64,
    /// For a directory: its canonical path followed by `/`.
    under: Option<Vec<u8>>,
}

impl Target {
    /// Whether the object `link` names (followed) is the target or under it.
    fn named_by(&self, link: &Path) -> bool {
        let same = fs::metadata(link)
            .is_ok_and(|meta| meta.dev() == self.device && meta.ino() == self.inode);
        same || self.under.as_deref().is_some_and(|dir| {
            fs::read_link(link).is_ok_and(|target| target.as_os_str().as_bytes().starts_with(dir))
        })
    }

    /// Whether a mapped path is the target or under it.
    fn mapped(&self, path: &[u8]) -> bool {
        self.under
            .as_deref()
            .is_some_and(|dir| path.starts_with(dir))
            || fs::metadata(Path::new(std::ffi::OsStr::from_bytes(path)))
                .is_ok_and(|meta| meta.dev() == self.device && meta.ino() == self.inode)
    }
}

/// What one process revealed.
enum Seen {
    Uses(Process),
    Clean,
    Hidden,
}

fn process(pid: u32, proc_dir: &Path) -> Process {
    Process {
        pid,
        started_at: None,
        executable: fs::read_link(proc_dir.join("exe"))
            .ok()
            .and_then(|exe| RawPath::from_unix_bytes(exe.as_os_str().as_bytes().to_vec()).ok()),
        executable_identity: None,
        application: None,
    }
}

/// The mapped file paths of a process (`/proc/<pid>/maps`, sixth column).
fn mapped_paths(proc_dir: &Path) -> Vec<Vec<u8>> {
    fs::read(proc_dir.join("maps"))
        .map(|maps| {
            maps.split(|&b| b == b'\n')
                .filter_map(|line| {
                    let start = line.iter().position(|&b| b == b'/')?;
                    line.get(start..).map(<[u8]>::to_vec)
                })
                .collect()
        })
        .unwrap_or_default()
}

fn inspect(pid: u32, target: &Target) -> Seen {
    let proc_dir = PathBuf::from(format!("/proc/{pid}"));
    let fds = match fs::read_dir(proc_dir.join("fd")) {
        Ok(fds) => fds,
        Err(error) if error.kind() == ErrorKind::NotFound => return Seen::Clean,
        Err(_) => return Seen::Hidden,
    };
    let uses = fds
        .filter_map(Result::ok)
        .any(|fd| target.named_by(&fd.path()))
        || ["cwd", "exe"]
            .iter()
            .any(|link| target.named_by(&proc_dir.join(link)))
        || mapped_paths(&proc_dir)
            .iter()
            .any(|path| target.mapped(path));
    if uses {
        Seen::Uses(process(pid, &proc_dir))
    } else {
        Seen::Clean
    }
}

impl InUseProbe for ProcProbe {
    fn probe(&self, path: &RawPath, expected: &FileIdentity) -> InUse {
        if path.flavor() != PathFlavor::Unix {
            return InUse::Unknown {
                reason: "not a Unix path",
            };
        }
        let native = Path::new(std::ffi::OsStr::from_bytes(path.as_bytes()));
        let Ok(meta) = fs::symlink_metadata(native) else {
            return InUse::Unknown {
                reason: "the object cannot be observed",
            };
        };
        // Same identity rule as the enumerators.
        let identity_matches = VolumeId::new(format!("dev:{:x}", meta.dev()))
            .is_ok_and(|volume| volume == expected.volume)
            && u128::from(meta.ino()) == expected.file.get();
        if !identity_matches {
            return InUse::Unknown {
                reason: "the object changed since it was observed",
            };
        }
        let under = if meta.is_dir() {
            match fs::canonicalize(native) {
                Ok(dir) => {
                    let mut prefix = dir.as_os_str().as_bytes().to_vec();
                    if !prefix.ends_with(b"/") {
                        prefix.push(b'/');
                    }
                    Some(prefix)
                }
                Err(_) => {
                    return InUse::Unknown {
                        reason: "the directory cannot be resolved",
                    };
                }
            }
        } else {
            None
        };
        let target = Target {
            device: meta.dev(),
            inode: meta.ino(),
            under,
        };
        let Ok(entries) = fs::read_dir("/proc") else {
            return InUse::Unknown {
                reason: "the process list cannot be read",
            };
        };
        let mut users = Vec::new();
        let mut unseen = 0_u32;
        for pid in entries
            .filter_map(Result::ok)
            .filter_map(|e| e.file_name().to_str()?.parse::<u32>().ok())
        {
            match inspect(pid, &target) {
                Seen::Uses(process) => users.push(process),
                Seen::Clean => {}
                Seen::Hidden => unseen = unseen.saturating_add(1),
            }
        }
        if !users.is_empty() {
            InUse::Yes(users)
        } else if unseen > 0 {
            InUse::Partial {
                unseen: Some(unseen),
            }
        } else {
            InUse::No
        }
    }
}

#[cfg(test)]
mod tests {
    use lumen_domain::FileId;
    use lumen_testkit::{Fixture, Node};

    use super::*;

    fn observed(path: &Path) -> (RawPath, FileIdentity) {
        let meta = fs::symlink_metadata(path).unwrap_or_else(|e| unreachable!("{e}"));
        (
            RawPath::from_unix_bytes(path.as_os_str().as_bytes().to_vec())
                .unwrap_or_else(|e| unreachable!("{e}")),
            FileIdentity {
                volume: VolumeId::new(format!("dev:{:x}", meta.dev()))
                    .unwrap_or_else(|e| unreachable!("{e}")),
                file: FileId::new(u128::from(meta.ino())),
            },
        )
    }

    fn me(answer: &InUse) -> bool {
        matches!(answer, InUse::Yes(users) if users.iter().any(|p| p.pid == std::process::id()))
    }

    #[test]
    fn a_file_this_process_holds_open_is_in_use() {
        let fx = Fixture::build(&[Node::File {
            path: "dir/held".into(),
            contents: b"x".to_vec(),
        }])
        .unwrap_or_else(|e| unreachable!("{e}"));
        let held = fx.path("dir/held").unwrap_or_else(|e| unreachable!("{e}"));
        let _open = fs::File::open(&held).unwrap_or_else(|e| unreachable!("{e}"));
        let (raw, identity) = observed(&held);
        assert!(me(&ProcProbe.probe(&raw, &identity)), "the file itself");
        let dir = fs::canonicalize(fx.path("dir").unwrap_or_else(|e| unreachable!("{e}")))
            .unwrap_or_else(|e| unreachable!("{e}"));
        let (raw, identity) = observed(&dir);
        assert!(
            me(&ProcProbe.probe(&raw, &identity)),
            "a file under the directory"
        );
    }

    #[test]
    fn an_unused_file_is_not_in_use_by_this_process() {
        let fx = Fixture::build(&[Node::File {
            path: "idle".into(),
            contents: b"x".to_vec(),
        }])
        .unwrap_or_else(|e| unreachable!("{e}"));
        let (raw, identity) = observed(&fx.path("idle").unwrap_or_else(|e| unreachable!("{e}")));
        let answer = ProcProbe.probe(&raw, &identity);
        assert!(
            matches!(answer, InUse::No | InUse::Partial { .. }),
            "{answer:?}"
        );
    }

    #[test]
    fn a_replaced_object_is_unknown() {
        let fx = Fixture::build(&[Node::File {
            path: "f".into(),
            contents: b"x".to_vec(),
        }])
        .unwrap_or_else(|e| unreachable!("{e}"));
        let (raw, mut identity) = observed(&fx.path("f").unwrap_or_else(|e| unreachable!("{e}")));
        identity.file = FileId::new(identity.file.get() ^ 1);
        assert!(matches!(
            ProcProbe.probe(&raw, &identity),
            InUse::Unknown { .. }
        ));
    }
}
