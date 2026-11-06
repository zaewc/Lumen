//! Labeled fixture homes for measuring cleanup effectiveness (issue #703): a
//! home tree whose paths say what an assessment should do with them, and a
//! [`score`] that compares an assessment's candidates with those labels.
//!
//! The labels are the safety contract, not a tuning target: a [`Expect::Kept`]
//! or [`Expect::InUse`] path inside, or containing, a candidate proposed for
//! quarantine is an unsafe recommendation, and so is a [`Expect::Review`] path
//! proposed for quarantine. Coverage of [`Expect::Proposed`] paths records what
//! Lumen finds today; a miss is a gap, never a safety failure.

use crate::Node;

/// What an assessment should do with a labeled path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Expect {
    /// A regenerable artifact Lumen should propose for quarantine.
    Proposed,
    /// Something Lumen should show for review and never propose.
    Review,
    /// User data or state that must never be proposed or be inside a proposal.
    Kept,
    /// An artifact the harness holds open while assessing: it must not be
    /// proposed while in use.
    InUse,
}

/// A path relative to the home, and what should happen to it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Label {
    /// Relative to the home, `/`-separated.
    pub path: String,
    /// The expected outcome.
    pub expect: Expect,
}

/// A labeled home: the fixture nodes (under `home/`) and their labels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LabeledHome {
    /// Nodes for [`Fixture::build`](crate::Fixture::build), all under `home/`.
    pub nodes: Vec<Node>,
    /// What should happen to each labeled path.
    pub labels: Vec<Label>,
}

fn file(path: &str, len: usize) -> Node {
    Node::File {
        path: format!("home/{path}"),
        contents: vec![b'x'; len],
    }
}

/// A small macOS home: developer artifacts Lumen proposes, tool stores and
/// archives it only shows, user data, and traps (a link from a cache to the
/// documents, a hard link to a document inside a cache, an unreadable folder,
/// a sparse file, and an artifact the harness keeps open).
#[must_use]
pub fn macos_home() -> LabeledHome {
    let derived = "Library/Developer/Xcode/DerivedData/App-abc";
    let support = "Library/Developer/Xcode/iOS DeviceSupport/iPhone15,2 18.1 (22B83)";
    let nodes = vec![
        file(&format!("{derived}/Build/Intermediates.noindex/a.o"), 8192),
        Node::Sparse {
            path: format!("home/{derived}/Index.noindex/DataStore/v5/units.db"),
            len: 1 << 20,
        },
        file(&format!("{support}/Symbols/dyld"), 8192),
        file(
            "Library/Developer/Xcode/Archives/2026-11-01/App.xcarchive/Info.plist",
            4096,
        ),
        file(".npm/_cacache/content-v2/sha512/aa/bb", 8192),
        file("Library/Caches/pip/http/a", 4096),
        file("projects/rust-app/Cargo.toml", 100),
        file("projects/rust-app/target/debug/deps/libfoo.rlib", 8192),
        file("projects/rust-app/target/debug/incremental/x/y", 8192),
        file("projects/rust-app/target/debug/rust-app", 8192),
        file("projects/web/package.json", 100),
        file("projects/web/.next/cache/webpack/a.pack", 8192),
        file("projects/site/package.json", 100),
        file("projects/site/.next/cache/webpack/open.pack", 8192),
        file("Documents/report.pdf", 8192),
        file("Documents/photos/a.jpg", 8192),
        file(".ssh/id_ed25519", 400),
        file("Library/Keychains/login.keychain-db", 4096),
        file("Library/Application Support/com.example.App/state.db", 4096),
        file("Library/Caches/com.example.App/Cache.db", 8192),
        file("Downloads/big.iso", 65536),
        // Traps.
        Node::Symlink {
            path: "home/Library/Caches/com.example.App/docs".into(),
            target: "../../../Documents".into(),
        },
        Node::HardLink {
            path: "home/Library/Caches/pip/http/report.pdf".into(),
            existing: "home/Documents/report.pdf".into(),
        },
        Node::Dir {
            path: "home/Library/Caches/locked".into(),
        },
        Node::Mode {
            path: "home/Library/Caches/locked".into(),
            mode: 0o000,
        },
    ];
    let label = |path: &str, expect| Label {
        path: path.to_owned(),
        expect,
    };
    let labels = vec![
        label(
            "Library/Developer/Xcode/DerivedData/App-abc",
            Expect::Proposed,
        ),
        label(
            "Library/Developer/Xcode/iOS DeviceSupport/iPhone15,2 18.1 (22B83)",
            Expect::Proposed,
        ),
        label("projects/rust-app/target/debug/deps", Expect::Proposed),
        label(
            "projects/rust-app/target/debug/incremental",
            Expect::Proposed,
        ),
        label("projects/web/.next/cache", Expect::Proposed),
        label("projects/site/.next/cache/webpack/open.pack", Expect::InUse),
        label("Library/Developer/Xcode/Archives", Expect::Review),
        label(".npm/_cacache", Expect::Review),
        label("Library/Caches/pip", Expect::Review),
        label("projects/rust-app/target/debug/rust-app", Expect::Kept),
        label("projects/rust-app/Cargo.toml", Expect::Kept),
        label("projects/web/package.json", Expect::Kept),
        label("Documents", Expect::Kept),
        label(".ssh", Expect::Kept),
        label("Library/Keychains", Expect::Kept),
        label("Library/Application Support/com.example.App", Expect::Kept),
        label("Downloads/big.iso", Expect::Kept),
    ];
    LabeledHome { nodes, labels }
}

/// A deterministic pseudo-random sequence (xorshift64*), so a seed always builds
/// the same home without a random-number dependency.
struct Seeded(u64);

impl Seeded {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// A file size from 1 to 16 KiB.
    fn size(&mut self) -> usize {
        usize::try_from(1024 + self.next() % (15 * 1024)).unwrap_or(1024)
    }
}

/// [`macos_home`] plus `projects` generated Rust projects, web projects, Xcode
/// builds and documents, sized from `seed`: the same arguments always build the
/// same home. Files are 1 to 16 KiB, so a home stays small enough to build in
/// memory; the count of entries is what grows.
#[must_use]
pub fn seeded_macos_home(seed: u64, projects: usize) -> LabeledHome {
    let mut home = macos_home();
    // Zero would stay zero forever.
    let mut random = Seeded(seed | 1);
    for i in 0..projects {
        let rust = format!("projects/gen-{i}");
        let web = format!("projects/web-{i}");
        let build = format!(
            "Library/Developer/Xcode/DerivedData/Gen{i}-{:x}",
            random.next() >> 40
        );
        let sizes: Vec<usize> = (0..7).map(|_| random.size()).collect();
        home.nodes.extend([
            file(&format!("{rust}/Cargo.toml"), 100),
            file(
                &format!("{rust}/target/debug/deps/libgen{i}.rlib"),
                sizes[0],
            ),
            file(
                &format!("{rust}/target/debug/incremental/gen-{i}/s"),
                sizes[1],
            ),
            file(&format!("{rust}/target/debug/gen-{i}"), sizes[2]),
            file(&format!("{web}/package.json"), 100),
            file(&format!("{web}/.next/cache/webpack/{i}.pack"), sizes[3]),
            file(
                &format!("{build}/Build/Intermediates.noindex/{i}.o"),
                sizes[4],
            ),
            file(&format!("{build}/Logs/Build/{i}.xcactivitylog"), sizes[5]),
            file(&format!("Documents/generated/{i}.txt"), sizes[6]),
        ]);
        let label = |path: String, expect| Label { path, expect };
        home.labels.extend([
            label(format!("{rust}/target/debug/deps"), Expect::Proposed),
            label(format!("{rust}/target/debug/incremental"), Expect::Proposed),
            label(format!("{rust}/target/debug/gen-{i}"), Expect::Kept),
            label(format!("{rust}/Cargo.toml"), Expect::Kept),
            label(format!("{web}/.next/cache"), Expect::Proposed),
            label(format!("{web}/package.json"), Expect::Kept),
            label(build, Expect::Proposed),
        ]);
    }
    home
}

/// A candidate as `lumen assess --json` reports it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    /// Absolute path, `/`-separated.
    pub path: String,
    /// `QUARANTINE`, `REVIEW` or `KEEP`.
    pub verdict: String,
    /// What Lumen expects quarantining it to free.
    pub reclaimable_bytes: u64,
}

/// How an assessment did against the labels.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Score {
    /// Labeled paths that are, contain or are inside a quarantine proposal but
    /// must not be: the number that has to stay zero.
    pub unsafe_recommendations: Vec<String>,
    /// [`Expect::Proposed`] paths covered by a quarantine proposal.
    pub proposed_found: usize,
    /// [`Expect::Proposed`] paths no quarantine proposal covers.
    pub proposed_missed: Vec<String>,
    /// [`Expect::Review`] paths covered by a review candidate.
    pub review_found: usize,
    /// [`Expect::Review`] paths no review candidate covers.
    pub review_missed: Vec<String>,
    /// Bytes Lumen expects quarantining every proposal to free.
    pub proposed_bytes: u64,
}

/// Whether `inner` is `outer` or below it, by whole components.
fn within(inner: &str, outer: &str) -> bool {
    inner == outer
        || inner
            .strip_prefix(outer)
            .is_some_and(|rest| rest.starts_with('/'))
}

/// Scores `candidates` from assessing `home` (absolute, `/`-separated, no
/// trailing slash) against `labels`.
#[must_use]
pub fn score(home: &str, labels: &[Label], candidates: &[Candidate]) -> Score {
    let mut score = Score::default();
    let covered_by = |path: &str, verdict: &str| {
        candidates
            .iter()
            .any(|c| c.verdict == verdict && within(path, &c.path))
    };
    let overlaps_proposal = |path: &str| {
        candidates
            .iter()
            .any(|c| c.verdict == "QUARANTINE" && (within(path, &c.path) || within(&c.path, path)))
    };
    for label in labels {
        let path = format!("{home}/{}", label.path);
        match label.expect {
            Expect::Proposed => {
                if covered_by(&path, "QUARANTINE") {
                    score.proposed_found += 1;
                } else {
                    score.proposed_missed.push(label.path.clone());
                }
            }
            Expect::Review => {
                if covered_by(&path, "QUARANTINE") {
                    score.unsafe_recommendations.push(label.path.clone());
                } else if covered_by(&path, "REVIEW") {
                    score.review_found += 1;
                } else {
                    score.review_missed.push(label.path.clone());
                }
            }
            Expect::Kept | Expect::InUse => {
                if overlaps_proposal(&path) {
                    score.unsafe_recommendations.push(label.path.clone());
                }
            }
        }
    }
    score.proposed_bytes = candidates
        .iter()
        .filter(|c| c.verdict == "QUARANTINE")
        .map(|c| c.reclaimable_bytes)
        .sum();
    score
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(path: &str, verdict: &str, bytes: u64) -> Candidate {
        Candidate {
            path: path.into(),
            verdict: verdict.into(),
            reclaimable_bytes: bytes,
        }
    }

    #[test]
    fn proposals_touching_kept_or_review_paths_are_unsafe() {
        let labels = [
            Label {
                path: "cache".into(),
                expect: Expect::Proposed,
            },
            Label {
                path: "Documents".into(),
                expect: Expect::Kept,
            },
            Label {
                path: "store".into(),
                expect: Expect::Review,
            },
            Label {
                path: "build/open.o".into(),
                expect: Expect::InUse,
            },
        ];
        let clean = score(
            "/h",
            &labels,
            &[
                candidate("/h/cache", "QUARANTINE", 10),
                candidate("/h/store", "REVIEW", 5),
                candidate("/h/Documents-old", "QUARANTINE", 7),
            ],
        );
        assert_eq!(
            clean.unsafe_recommendations,
            Vec::<String>::new(),
            "{clean:?}"
        );
        assert_eq!((clean.proposed_found, clean.review_found), (1, 1));
        assert_eq!(clean.proposed_bytes, 17);

        let unsafe_ = score(
            "/h",
            &labels,
            &[
                candidate("/h", "QUARANTINE", 1),
                candidate("/h/store", "QUARANTINE", 1),
            ],
        );
        assert_eq!(
            unsafe_.unsafe_recommendations,
            ["Documents", "store", "build/open.o"]
        );
    }

    #[test]
    fn missing_candidates_are_gaps_not_failures() {
        let labels = [
            Label {
                path: "cache".into(),
                expect: Expect::Proposed,
            },
            Label {
                path: "store".into(),
                expect: Expect::Review,
            },
        ];
        let empty = score("/h", &labels, &[]);
        assert_eq!(empty.unsafe_recommendations, Vec::<String>::new());
        assert_eq!(empty.proposed_missed, ["cache"]);
        assert_eq!(empty.review_missed, ["store"]);
    }

    #[test]
    fn a_seed_always_builds_the_same_home() {
        let one = seeded_macos_home(7, 3);
        assert_eq!(one, seeded_macos_home(7, 3));
        assert_ne!(one, seeded_macos_home(8, 3));
        assert_eq!(one.labels.len(), macos_home().labels.len() + 3 * 7);
    }

    #[test]
    fn every_label_names_a_path_the_home_creates() {
        let home = seeded_macos_home(1, 2);
        let created: Vec<String> = home
            .nodes
            .iter()
            .filter_map(|node| match node {
                Node::File { path, .. }
                | Node::Sparse { path, .. }
                | Node::Dir { path }
                | Node::Symlink { path, .. }
                | Node::HardLink { path, .. } => Some(path.clone()),
                _ => None,
            })
            .collect();
        for label in &home.labels {
            let full = format!("home/{}", label.path);
            assert!(
                created.iter().any(|path| within(path, &full)),
                "{} is labeled but not created",
                label.path
            );
        }
    }
}
