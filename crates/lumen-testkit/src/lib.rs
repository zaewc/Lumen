//! Test support: declarative fixture directory trees.
//!
//! Every test that touches a filesystem uses a [`Fixture`]: a fresh temporary
//! directory populated from a list of [`Node`]s. Destructive behaviour is only ever
//! tested inside such fixtures, never on a real filesystem (AGENTS.md).
//!
//! Safety properties:
//!
//! - fixture paths must be relative and may not contain `..`, so a test can never
//!   write outside its temporary directory;
//! - the directory is removed when the [`Fixture`] is dropped, by `tempfile`
//!   (no destructive code in this crate).

#![forbid(unsafe_code)]

use std::fs::{self, File, FileTimes};
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, SystemTime};

use lumen_domain::Timestamp;

/// One object in a fixture tree. Paths are relative to the fixture root and use
/// `/` as separator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Node {
    /// A directory (parents are created as needed).
    Dir {
        /// Relative path.
        path: String,
    },
    /// A regular file with exact contents.
    File {
        /// Relative path.
        path: String,
        /// Contents.
        contents: Vec<u8>,
    },
    /// A file of `len` bytes created by extending an empty file, which most
    /// filesystems store sparsely (logical size greater than allocated size).
    Sparse {
        /// Relative path.
        path: String,
        /// Logical length.
        len: u64,
    },
    /// A symbolic link to `target` (stored verbatim; may dangle). Unix only.
    Symlink {
        /// Relative path of the link.
        path: String,
        /// Link target, stored as given.
        target: String,
    },
    /// A hard link to an existing fixture file.
    HardLink {
        /// Relative path of the new link.
        path: String,
        /// Relative path of the existing file.
        existing: String,
    },
    /// Sets the modification time of an existing fixture **file**.
    Modified {
        /// Relative path.
        path: String,
        /// New modification time.
        at: Timestamp,
    },
    /// Sets Unix permission bits on an existing fixture object (e.g. `0o000` to
    /// make a directory unreadable). Restored to `0o700` before the fixture is
    /// removed. Unix only.
    Mode {
        /// Relative path.
        path: String,
        /// Permission bits.
        mode: u32,
    },
}

/// Errors while building a fixture.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum FixtureError {
    /// A fixture path is absolute, empty, or contains `..`, `.` or a prefix.
    #[error("fixture path {path:?} must be relative, non-empty, and stay inside the fixture")]
    UnsafePath {
        /// Offending path.
        path: String,
    },
    /// The node is not supported on this platform.
    #[error("fixture node {node:?} is not supported on this platform")]
    Unsupported {
        /// Offending node.
        node: Box<Node>,
    },
    /// The timestamp is before the Unix epoch, which some filesystems reject.
    #[error("fixture times must not precede 1970-01-01")]
    PreEpochTime,
    /// An I/O operation failed.
    #[error("fixture I/O failed for {path:?}: {source}")]
    Io {
        /// Path being created.
        path: PathBuf,
        /// Underlying error.
        source: std::io::Error,
    },
}

/// A populated temporary directory. Removed when dropped.
#[derive(Debug)]
pub struct Fixture {
    dir: tempfile::TempDir,
    /// Paths whose permissions were restricted; restored before removal so the
    /// temporary directory can always be cleaned up.
    restricted: Vec<PathBuf>,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        for path in self.restricted.iter().rev() {
            // Drop cannot report errors; a failure here only leaves a temporary
            // directory behind, which the OS cleans up eventually.
            let restored = restore_mode(path);
            debug_assert!(
                restored.is_ok(),
                "could not restore permissions on {}",
                path.display()
            );
        }
    }
}

impl Fixture {
    /// Creates a fresh temporary directory and builds `nodes` in order.
    ///
    /// # Errors
    ///
    /// Returns a [`FixtureError`] if a path is unsafe, a node is unsupported, or
    /// I/O fails. No file is written outside the temporary directory.
    pub fn build(nodes: &[Node]) -> Result<Self, FixtureError> {
        let dir = tempfile::Builder::new()
            .prefix("lumen-fixture-")
            .tempdir()
            .map_err(|source| FixtureError::Io {
                path: std::env::temp_dir(),
                source,
            })?;
        let mut fixture = Self {
            dir,
            restricted: Vec::new(),
        };
        for node in nodes {
            fixture.create(node)?;
        }
        Ok(fixture)
    }

    /// The fixture root.
    pub fn root(&self) -> &Path {
        self.dir.path()
    }

    /// Absolute path of a fixture-relative path.
    ///
    /// # Errors
    ///
    /// Returns [`FixtureError::UnsafePath`] for paths that would leave the fixture.
    pub fn path(&self, relative: &str) -> Result<PathBuf, FixtureError> {
        Ok(self.root().join(safe_relative(relative)?))
    }

    fn create(&mut self, node: &Node) -> Result<(), FixtureError> {
        let io = |path: &Path| {
            let path = path.to_path_buf();
            move |source| FixtureError::Io { path, source }
        };
        match node {
            Node::Dir { path } => {
                let full = self.path(path)?;
                fs::create_dir_all(&full).map_err(io(&full))
            }
            Node::File { path, contents } => {
                let full = self.prepared(path)?;
                fs::write(&full, contents).map_err(io(&full))
            }
            Node::Sparse { path, len } => {
                let full = self.prepared(path)?;
                let file = File::create_new(&full).map_err(io(&full))?;
                file.set_len(*len).map_err(io(&full))
            }
            Node::Symlink { path, target } => {
                let full = self.prepared(path)?;
                symlink(target, &full, node)
            }
            Node::HardLink { path, existing } => {
                let original = self.path(existing)?;
                let full = self.prepared(path)?;
                fs::hard_link(&original, &full).map_err(io(&full))
            }
            Node::Modified { path, at } => {
                let full = self.path(path)?;
                let secs =
                    u64::try_from(at.unix_seconds()).map_err(|_| FixtureError::PreEpochTime)?;
                let time = SystemTime::UNIX_EPOCH + Duration::new(secs, at.subsec_nanos());
                let file = File::options().write(true).open(&full).map_err(io(&full))?;
                file.set_times(FileTimes::new().set_modified(time))
                    .map_err(io(&full))
            }
            Node::Mode { path, mode } => {
                let full = self.path(path)?;
                set_mode(&full, *mode, node)?;
                self.restricted.push(full);
                Ok(())
            }
        }
    }

    /// Resolves a path for a new object and creates its parent directories.
    fn prepared(&self, relative: &str) -> Result<PathBuf, FixtureError> {
        let full = self.path(relative)?;
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent).map_err(|source| FixtureError::Io {
                path: parent.to_path_buf(),
                source,
            })?;
        }
        Ok(full)
    }
}

#[cfg(unix)]
fn symlink(target: &str, link: &Path, _node: &Node) -> Result<(), FixtureError> {
    std::os::unix::fs::symlink(target, link).map_err(|source| FixtureError::Io {
        path: link.to_path_buf(),
        source,
    })
}

// Changing permissions is banned outside the executor (ADR-0016). Here it only
// ever touches objects inside the fixture's own temporary directory.
#[cfg(unix)]
#[allow(clippy::disallowed_methods)]
fn set_mode(path: &Path, mode: u32, _node: &Node) -> Result<(), FixtureError> {
    use std::os::unix::fs::PermissionsExt as _;
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).map_err(|source| FixtureError::Io {
        path: path.to_path_buf(),
        source,
    })
}

#[cfg(unix)]
#[allow(clippy::disallowed_methods)] // See set_mode.
fn restore_mode(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt as _;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
}

#[cfg(not(unix))]
fn set_mode(_path: &Path, _mode: u32, node: &Node) -> Result<(), FixtureError> {
    Err(FixtureError::Unsupported {
        node: Box::new(node.clone()),
    })
}

#[cfg(not(unix))]
#[allow(clippy::unnecessary_wraps)] // Same signature as the Unix version.
fn restore_mode(_path: &Path) -> std::io::Result<()> {
    Ok(())
}

#[cfg(not(unix))]
fn symlink(_target: &str, _link: &Path, node: &Node) -> Result<(), FixtureError> {
    // Windows symlinks need Developer Mode or elevation; Windows link fixtures
    // (junctions, symlinks) arrive with the Windows adapter.
    Err(FixtureError::Unsupported {
        node: Box::new(node.clone()),
    })
}

/// Validates a fixture-relative path: non-empty, relative, only normal
/// components.
fn safe_relative(relative: &str) -> Result<PathBuf, FixtureError> {
    let unsafe_path = || FixtureError::UnsafePath {
        path: relative.to_owned(),
    };
    let path = Path::new(relative);
    // `Path::components` silently drops interior `.` and empty segments, so the
    // string is checked as well: every `/`-separated segment must be a plain name.
    let plain_segments = relative
        .split(['/', '\\'])
        .all(|s| !s.is_empty() && s != "." && s != "..");
    if !plain_segments
        || path
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err(unsafe_path());
    }
    Ok(path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn file(path: &str, contents: &[u8]) -> Node {
        Node::File {
            path: path.into(),
            contents: contents.to_vec(),
        }
    }

    #[test]
    fn builds_files_dirs_and_parents() -> Result<(), FixtureError> {
        let fx = Fixture::build(&[
            Node::Dir {
                path: "empty".into(),
            },
            file("Library/Caches/com.example/data.bin", b"hello"),
        ])?;
        assert!(fx.path("empty")?.is_dir());
        assert_eq!(
            fs::read(fx.path("Library/Caches/com.example/data.bin")?)
                .ok()
                .as_deref(),
            Some(&b"hello"[..])
        );
        Ok(())
    }

    #[test]
    fn rejects_paths_that_escape_the_fixture() {
        for bad in ["", "/etc/passwd", "../outside", "a/../../b", "./a", "a/./b"] {
            assert!(
                matches!(
                    Fixture::build(&[file(bad, b"x")]),
                    Err(FixtureError::UnsafePath { .. })
                ),
                "{bad:?} must be rejected"
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn hard_links_share_identity_and_symlinks_are_not_followed() -> Result<(), FixtureError> {
        use std::os::unix::fs::MetadataExt as _;

        let fx = Fixture::build(&[
            file("a.txt", b"data"),
            Node::HardLink {
                path: "b.txt".into(),
                existing: "a.txt".into(),
            },
            Node::Symlink {
                path: "link".into(),
                target: "a.txt".into(),
            },
            Node::Symlink {
                path: "dangling".into(),
                target: "missing".into(),
            },
        ])?;
        let meta = |p: &str| {
            fx.path(p).and_then(|p| {
                fs::symlink_metadata(&p).map_err(|source| FixtureError::Io { path: p, source })
            })
        };
        let (a, b) = (meta("a.txt")?, meta("b.txt")?);
        assert_eq!((a.ino(), a.nlink()), (b.ino(), 2));
        assert!(meta("link")?.file_type().is_symlink());
        assert!(meta("dangling")?.file_type().is_symlink());
        Ok(())
    }

    #[test]
    fn sets_modification_time_and_sparse_length() -> Result<(), Box<dyn std::error::Error>> {
        let at: Timestamp = "2025-01-02T03:04:05Z".parse()?;
        let fx = Fixture::build(&[
            Node::Sparse {
                path: "big.img".into(),
                len: 64 << 20,
            },
            Node::Modified {
                path: "big.img".into(),
                at,
            },
        ])?;
        let meta = fs::metadata(fx.path("big.img")?)?;
        assert_eq!(meta.len(), 64 << 20);
        assert_eq!(
            meta.modified()?
                .duration_since(SystemTime::UNIX_EPOCH)?
                .as_secs(),
            1_735_787_045
        );
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn restricted_directories_are_unreadable_and_still_cleaned_up() -> Result<(), FixtureError> {
        let root = {
            let fx = Fixture::build(&[
                file("locked/secret.txt", b"x"),
                Node::Mode {
                    path: "locked".into(),
                    mode: 0o000,
                },
            ])?;
            let listing = fs::read_dir(fx.path("locked")?);
            // Root bypasses permission checks; only assert denial for ordinary users.
            if !running_as_root() {
                assert!(listing.is_err(), "a 0o000 directory must not be listable");
            }
            fx.root().to_path_buf()
        };
        assert!(
            !root.exists(),
            "fixture must be removed even with restricted permissions"
        );
        Ok(())
    }

    #[cfg(unix)]
    fn running_as_root() -> bool {
        use std::os::unix::fs::MetadataExt as _;
        // A file we create is owned by our effective UID.
        tempfile::tempfile()
            .and_then(|f| f.metadata())
            .is_ok_and(|m| m.uid() == 0)
    }

    #[test]
    fn fixture_is_removed_on_drop() -> Result<(), FixtureError> {
        let root = {
            let fx = Fixture::build(&[file("x", b"1")])?;
            fx.root().to_path_buf()
        };
        assert!(!root.exists());
        Ok(())
    }

    proptest! {
        #[test]
        fn accepted_paths_have_only_normal_components(segments in prop::collection::vec("[a-zA-Z0-9_ -]{1,8}|\\.\\.|\\.", 1..5)) {
            let relative = segments.join("/");
            match safe_relative(&relative) {
                Ok(path) => prop_assert!(path.components().all(|c| matches!(c, Component::Normal(_)))),
                Err(_) => prop_assert!(segments.iter().any(|s| s == ".." || s == ".")),
            }
        }

        #[test]
        fn dot_segments_are_always_rejected(prefix in "[a-z]{1,6}", dots in prop_oneof![Just(".."), Just(".")]) {
            prop_assert!(safe_relative(&[prefix.as_str(), dots, "x"].join("/")).is_err());
            prop_assert!(safe_relative(&[dots, prefix.as_str()].join("/")).is_err());
        }
    }
}
