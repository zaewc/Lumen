//! Installed RPM packages (#806, ADR-0060) and what each one requires
//! (ADR-0064), from `rpm -qa` with a fixed query format. RPM keeps its
//! database as SQLite, Berkeley DB or NDB depending on the distribution and
//! release; asking `rpm` itself reads whichever is in use.
//!
//! A requirement names a capability: a package name, a name packages provide
//! (a library's soname, `config(...)`), or a file. A file is met by the
//! package that owns it, so the required files are asked of `rpm` once more
//! (`--whatprovides`); a file no package is found to own leaves the requiring
//! package's relations unknown. `rpmlib(...)` requirements are met by `rpm`
//! itself and left out.
//!
//! The queries are read-only. The program is run by absolute path
//! (`/usr/bin/rpm`), with fixed arguments (and the required files after
//! `--`), no shell, closed input, a time limit and a bounded output. Without
//! that program the source is
//! [`SourceStatus::Unavailable`](lumen_domain::SourceStatus).

use std::collections::{BTreeSet, HashMap};

use lumen_domain::{Package, PackageManager, RelationMethod, UntrustedText};

use crate::relations::{Declared, link};

/// The query format: per package, a line of name, `epoch:version-release`
/// and architecture, then an `R` line per requirement and a `P` line per
/// provided capability ([rpm-queryformat(7)]).
///
/// [rpm-queryformat(7)]: https://rpm-software-management.github.io/rpm/man/rpm-queryformat.7
pub const QUERY_FORMAT: &str = "%{NAME}\\t%{EPOCHNUM}:%{VERSION}-%{RELEASE}\\t%{ARCH}\\n\
                                [R\\t%{REQUIRENAME}\\n][P\\t%{PROVIDENAME}\\n]";

/// The query format without requirements and provided capabilities, asked
/// only if the full query does not finish within its limits: the list is kept
/// and every package's relations are unknown.
pub const LIST_FORMAT: &str = "%{NAME}\\t%{EPOCHNUM}:%{VERSION}-%{RELEASE}\\t%{ARCH}\\n";

/// The query format for the owners of required files: a line per file of
/// each owning package, its `name.arch` and the file.
pub const OWNERS_FORMAT: &str = "[%{=NAME}.%{=ARCH}\\t%{FILENAMES}\\n]";

/// Whether `text` can be a capability: not empty, no spaces or control
/// characters. Boolean dependency expressions are not interpreted.
fn is_capability(text: &str) -> bool {
    !text.is_empty() && !text.chars().any(|c| c.is_whitespace() || c.is_control())
}

/// One package's lines, as read.
struct Record<'a> {
    package: Package,
    name: &'a str,
    requires: Vec<&'a str>,
    provides: Vec<&'a str>,
}

/// Each package line with its `R` and `P` lines, and how many lines could not
/// be read. `gpg-pubkey` entries, whose architecture is `(none)`, are signing
/// keys, not software, and are left out.
fn records(output: &str) -> (Vec<Record<'_>>, usize) {
    let mut records: Vec<Record<'_>> = Vec::new();
    let mut malformed = 0;
    for line in output.lines().filter(|l| !l.trim().is_empty()) {
        let fields: Vec<&str> = line.split('\t').collect();
        let last = records.last_mut();
        match (fields.as_slice(), last) {
            (&["R", name], Some(record)) => record.requires.push(name),
            (&["P", name], Some(record)) => record.provides.push(name),
            (&[name, version, arch], _) => {
                let (name, version, arch) = (name.trim(), version.trim(), arch.trim());
                if name.is_empty() || arch.is_empty() {
                    malformed += 1;
                    continue;
                }
                let version = version.strip_prefix("0:").unwrap_or(version);
                records.push(Record {
                    package: Package {
                        manager: PackageManager::Rpm,
                        id: UntrustedText::new(format!("{name}.{arch}")),
                        version: Some(version)
                            .filter(|v| !v.is_empty())
                            .map(UntrustedText::new),
                        install_root: None,
                        // RPM has no "part of the base system" mark; dnf's
                        // protected packages are configuration, not package
                        // metadata.
                        required: false,
                        // RPM does not record why a package was installed.
                        explicit: None,
                        relations: None,
                    },
                    name,
                    requires: Vec::new(),
                    provides: Vec::new(),
                });
            }
            _ => malformed += 1,
        }
    }
    records.retain(|r| !r.package.id.raw().ends_with(".(none)"));
    (records, malformed)
}

/// The files that `rpm -qa` output in [`QUERY_FORMAT`] requires, to ask their
/// owners in [`OWNERS_FORMAT`].
#[must_use]
pub fn required_files(output: &str) -> Vec<String> {
    let (records, _) = records(output);
    let files: BTreeSet<&str> = records
        .iter()
        .flat_map(|r| &r.requires)
        .filter(|name| name.starts_with('/') && is_capability(name))
        .copied()
        .collect();
    files.into_iter().map(str::to_owned).collect()
}

/// The packages in `rpm -qa` output in [`QUERY_FORMAT`], and how many lines
/// could not be read. `owners` is the output for [`required_files`] in
/// [`OWNERS_FORMAT`], or `None` where it was not read.
///
/// A package is identified as RPM does, `name.arch`, so the two architectures
/// of a multilib package stay apart. An epoch of 0 is not shown.
///
/// Relations are matched as dpkg's are (see [`crate::relations`]), a file
/// counting as provided by the package that owns it. They are known only
/// where every line was read and every provided capability is one. A package
/// with a requirement that cannot be read, or that names a file no package
/// is found to own, has unknown relations.
#[must_use]
pub fn parse_rpm_query(output: &str, owners: Option<&str>) -> (Vec<Package>, usize) {
    let (records, malformed) = records(output);
    let mut index: HashMap<&str, Vec<usize>> = HashMap::new();
    for (i, record) in records.iter().enumerate() {
        index.entry(record.package.id.raw()).or_default().push(i);
    }
    let mut owned: Vec<Vec<&str>> = vec![Vec::new(); records.len()];
    // Whole lines only: a cut-off last line could name a shorter path.
    let whole = owners.map(|o| o.rsplit_once('\n').map_or("", |(lines, _)| lines));
    for line in whole.into_iter().flat_map(str::lines) {
        if let Some((id, file)) = line.split_once('\t')
            && let Some(found) = index.get(id)
        {
            for &i in found {
                owned[i].push(file);
            }
        }
    }
    let provided: BTreeSet<&str> = records
        .iter()
        .flat_map(|r| r.provides.iter().copied())
        .chain(owned.iter().flatten().copied())
        .collect();
    let provides_read = records
        .iter()
        .all(|r| r.provides.iter().all(|p| is_capability(p)));
    let needs = |requires: &[&str]| -> Option<Vec<Vec<String>>> {
        requires
            .iter()
            .filter(|name| !name.starts_with("rpmlib("))
            .map(|name| {
                if name.starts_with('(') {
                    return None;
                }
                let names = is_capability(name).then(|| vec![(*name).to_owned()])?;
                let unowned = |n: &String| n.starts_with('/') && !provided.contains(n.as_str());
                (!names.iter().any(unowned)).then_some(names)
            })
            .collect()
    };
    let declarations: Vec<Declared> = records
        .iter()
        .zip(&owned)
        .map(|(record, files)| Declared {
            name: record.name.to_owned(),
            // This query omits version bounds: a name alone cannot prove
            // this package satisfies its own requirement.
            allow_self_satisfaction: false,
            provides: record
                .provides
                .iter()
                .chain(files)
                .map(|p| (*p).to_owned())
                .collect(),
            needs: needs(&record.requires),
        })
        .collect();
    let mut packages: Vec<Package> = records.into_iter().map(|r| r.package).collect();
    if malformed == 0 && provides_read {
        link(&mut packages, &declarations, RelationMethod::RpmDatabase);
    }
    (packages, malformed)
}

#[cfg(target_os = "linux")]
pub use inventory::RpmInventory;

#[cfg(target_os = "linux")]
mod inventory {
    use std::io::Read as _;
    use std::path::Path;
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    use lumen_application::CancelToken;
    use lumen_application::ports::{InventoryReport, InventorySource};
    use lumen_domain::{Package, SourceName, SourceStatus};

    use super::{LIST_FORMAT, OWNERS_FORMAT, QUERY_FORMAT, parse_rpm_query, required_files};

    const PROGRAM: &str = "/usr/bin/rpm";
    const TIME_LIMIT: Duration = Duration::from_secs(60);
    const OUTPUT_LIMIT: u64 = 32 * 1024 * 1024;
    /// The most required files asked about; past it, their owners are not
    /// asked and the packages requiring files have unknown relations.
    const MAX_FILES: usize = 2048;

    /// Installed RPM packages, source `linux.rpm`.
    #[derive(Debug, Clone, Copy, Default)]
    pub struct RpmInventory;

    /// The output of `rpm` with `args`, or `None` if it did not finish within
    /// the limits, or did not succeed and `must_succeed`. (`--whatprovides`
    /// fails when any file has no owner, and still lists the others.)
    fn query(args: &[&str], must_succeed: bool, cancel: &CancelToken) -> Option<String> {
        let mut child = Command::new(PROGRAM)
            .args(args)
            .env_clear()
            .env("LC_ALL", "C")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;
        let stdout = child.stdout.take()?;
        // Read on its own thread so a full pipe never stalls the wait below.
        let reader = std::thread::spawn(move || {
            let mut bytes = Vec::new();
            let read = stdout.take(OUTPUT_LIMIT + 1).read_to_end(&mut bytes);
            read.ok()
                .filter(|n| *n as u64 <= OUTPUT_LIMIT)
                .map(|_| bytes)
        });
        let started = Instant::now();
        let status = loop {
            if let Ok(Some(status)) = child.try_wait() {
                break Some(status);
            }
            if cancel.is_cancelled() || started.elapsed() > TIME_LIMIT {
                let _killed = child.kill();
                let _reaped = child.wait();
                break None;
            }
            std::thread::sleep(Duration::from_millis(20));
        };
        let bytes = reader.join().ok()??;
        status.filter(|s| s.success() || !must_succeed)?;
        String::from_utf8(bytes).ok()
    }

    /// The owners of `files`, or `None` if not asked or not read.
    fn owners(files: &[String], cancel: &CancelToken) -> Option<String> {
        if files.is_empty() {
            return Some(String::new());
        }
        if files.len() > MAX_FILES {
            return None;
        }
        let mut args = vec!["-q", "--whatprovides", "--queryformat", OWNERS_FORMAT, "--"];
        args.extend(files.iter().map(String::as_str));
        query(&args, false, cancel)
    }

    impl InventorySource for RpmInventory {
        fn name(&self) -> SourceName {
            SourceName::new("linux.rpm").unwrap_or_else(|_| unreachable!("a valid literal"))
        }

        fn collect(&self, cancel: &CancelToken) -> InventoryReport {
            let report = |status, packages: Vec<Package>| InventoryReport {
                source: self.name(),
                status,
                applications: Vec::new(),
                services: Vec::new(),
                packages,
            };
            if !Path::new(PROGRAM).exists() {
                return report(SourceStatus::Unavailable, Vec::new());
            }
            let parsed = if let Some(output) =
                query(&["-qa", "--queryformat", QUERY_FORMAT], true, cancel)
            {
                let owners = owners(&required_files(&output), cancel);
                Some(parse_rpm_query(&output, owners.as_deref()))
            } else if cancel.is_cancelled() {
                None
            } else {
                query(&["-qa", "--queryformat", LIST_FORMAT], true, cancel).map(|output| {
                    let (mut packages, malformed) = parse_rpm_query(&output, None);
                    for package in &mut packages {
                        package.relations = None;
                    }
                    (packages, malformed)
                })
            };
            match parsed {
                None => report(SourceStatus::Partial, Vec::new()),
                Some((packages, malformed)) => {
                    let status = if malformed == 0 {
                        SourceStatus::Complete
                    } else {
                        SourceStatus::Partial
                    };
                    report(status, packages)
                }
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use lumen_application::CancelToken;
        use lumen_application::ports::InventorySource as _;
        use lumen_domain::SourceStatus;

        use super::{PROGRAM, RpmInventory};

        #[test]
        fn rpm_is_asked_only_where_it_is_installed() {
            // CI's Linux runner is Ubuntu, usually without RPM.
            let report = RpmInventory.collect(&CancelToken::new());
            assert_eq!(report.source.as_str(), "linux.rpm");
            if std::path::Path::new(PROGRAM).exists() {
                assert_ne!(report.status, SourceStatus::Unavailable);
            } else {
                assert_eq!(report.status, SourceStatus::Unavailable);
                assert_eq!(report.packages.len(), 0);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use lumen_domain::PackageRelationKind;

    use super::*;

    #[test]
    fn rpm_output_becomes_packages_by_name_and_architecture() {
        let output = "\
bash\t0:5.2.26-3.fc40\tx86_64
glibc\t0:2.39-22.fc40\tx86_64
glibc\t0:2.39-22.fc40\ti686
tzdata\t0:2024a-5.fc40\tnoarch
NetworkManager\t1:1.46.2-1.fc40\tx86_64
gpg-pubkey\t0:a15b79cc-63d04c2c\t(none)
broken line
\t0:1-1\tx86_64
";
        let (packages, malformed) = parse_rpm_query(output, Some(""));
        let found: Vec<_> = packages
            .iter()
            .map(|p| {
                (
                    p.id.raw().to_owned(),
                    p.version.as_ref().map(|v| v.raw().to_owned()),
                )
            })
            .collect();
        assert_eq!(
            found,
            [
                ("bash.x86_64".into(), Some("5.2.26-3.fc40".into())),
                ("glibc.x86_64".into(), Some("2.39-22.fc40".into())),
                ("glibc.i686".into(), Some("2.39-22.fc40".into())),
                ("tzdata.noarch".into(), Some("2024a-5.fc40".into())),
                (
                    "NetworkManager.x86_64".into(),
                    Some("1:1.46.2-1.fc40".into())
                ),
            ]
        );
        assert!(
            packages
                .iter()
                .all(|p| p.manager == PackageManager::Rpm && !p.required && p.explicit.is_none())
        );
        assert_eq!(
            malformed, 2,
            "a line without three fields and one without a name"
        );
        assert!(
            packages.iter().all(|p| p.relations.is_none()),
            "lines not read: relations unknown"
        );
    }

    const LINKED: &str = "\
bash\t0:5.2.26-3.fc40\tx86_64
R\t/bin/sh
R\tlibc.so.6()(64bit)
R\trpmlib(CompressedFileNames)
P\t/bin/sh
P\tbash
P\tbash(x86-64)
glibc\t0:2.39-22.fc40\tx86_64
P\tlibc.so.6()(64bit)
glibc\t0:2.39-22.fc40\ti686
P\tlibc.so.6
python3\t0:3.12.4-1.fc40\tx86_64
R\t/usr/bin/python3
R\tglibc
python3-tool\t0:1-1.fc40\tnoarch
R\t/usr/bin/python3
R\t(python3 >= 3.12 or pypy3)
R\t(glibc if bash)
legacy\t0:1-1.fc40\tnoarch
R\t/usr/bin/perl
gpg-pubkey\t0:a15b79cc-63d04c2c\t(none)
P\tgpg(Fedora)
";

    const OWNERS: &str = "\
python3.x86_64\t/usr/bin/python3
python3.x86_64\t/usr/bin/pydoc3
bash.x86_64\t/usr/bin/bash
glibc.x86_64\t/usr/bin/perl-cut";

    /// Each relation of a package: `(single, ids)`.
    fn relations(package: &Package) -> Option<Vec<(bool, Vec<&str>)>> {
        package.relations.as_ref().map(|relations| {
            relations
                .iter()
                .map(|r| {
                    assert_eq!(r.method, RelationMethod::RpmDatabase);
                    match &r.kind {
                        PackageRelationKind::DependsOn { package, .. } => {
                            (true, vec![package.id.raw()])
                        }
                        PackageRelationKind::DependsOnOneOf { packages, .. } => {
                            (false, packages.iter().map(|p| p.id.raw()).collect())
                        }
                        other => unreachable!("{other:?}"),
                    }
                })
                .collect()
        })
    }

    #[test]
    fn requirements_are_matched_to_providers_and_file_owners() {
        assert_eq!(
            required_files(LINKED),
            ["/bin/sh", "/usr/bin/perl", "/usr/bin/python3"]
        );
        let (packages, malformed) = parse_rpm_query(LINKED, Some(OWNERS));
        assert_eq!(malformed, 0);
        let all: Vec<_> = packages.iter().map(relations).collect();
        assert_eq!(
            all,
            [
                // Self candidates stay visible because version bounds are not read.
                Some(vec![
                    (false, vec!["bash.x86_64"]),
                    (false, vec!["glibc.x86_64"])
                ]),
                Some(Vec::new()),
                Some(Vec::new()),
                // Two packages of that name: either may meet it.
                Some(vec![
                    (false, vec!["python3.x86_64"]),
                    (false, vec!["glibc.x86_64", "glibc.i686"])
                ]),
                // Boolean dependencies are not flattened into guessed alternatives.
                None,
                // No owner found for `/usr/bin/perl` (a cut-off line is not
                // read): unknown.
                None,
            ]
        );
    }

    #[test]
    fn without_the_file_owners_packages_requiring_files_are_unknown() {
        let (packages, _) = parse_rpm_query(LINKED, None);
        let known: Vec<bool> = packages.iter().map(|p| p.relations.is_some()).collect();
        // bash provides `/bin/sh` itself.
        assert_eq!(known, [true, true, true, false, false, false]);
    }

    #[test]
    fn an_unreadable_capability_leaves_relations_unknown() {
        let bad_provides = LINKED.replace("P\tbash\n", "P\tba sh\n");
        let (packages, _) = parse_rpm_query(&bad_provides, Some(OWNERS));
        assert!(packages.iter().all(|p| p.relations.is_none()));
        let bad_rich = LINKED.replace("(glibc if bash)", "(glibc if bash");
        let (packages, _) = parse_rpm_query(&bad_rich, Some(OWNERS));
        assert!(packages[4].relations.is_none());
        assert!(packages[3].relations.is_some());
    }

    #[test]
    fn boolean_dependencies_remain_unknown() {
        for requirement in [
            "(foo or bar)",
            "(libc.so.6(GLIBC_2.34)(64bit) or bar)",
            "(foo if bar)",
            "(foo or)",
        ] {
            let output = format!("impl\t0:1-1\tx86_64\nR\t{requirement}\n");
            let (packages, malformed) = parse_rpm_query(&output, Some(""));
            assert_eq!(malformed, 0);
            assert!(packages[0].relations.is_none(), "{requirement}");
        }
    }

    #[test]
    fn empty_output_lists_nothing() {
        assert_eq!(parse_rpm_query("", None), (Vec::new(), 0));
    }
}
