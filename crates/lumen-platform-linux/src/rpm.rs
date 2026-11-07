//! Installed RPM packages (#806, ADR-0060), from `rpm -qa` with a fixed query
//! format. RPM keeps its database as SQLite, Berkeley DB or NDB depending on
//! the distribution and release; asking `rpm` itself reads whichever is in use.
//!
//! The query is read-only. The program is run by absolute path
//! (`/usr/bin/rpm`), with fixed arguments, no shell, closed input, a time limit
//! and a bounded output. Without that program the source is
//! [`SourceStatus::Unavailable`](lumen_domain::SourceStatus).

use lumen_domain::{Package, PackageManager, UntrustedText};

/// The query format: name, `epoch:version-release`, architecture.
pub const QUERY_FORMAT: &str = "%{NAME}\\t%{EPOCHNUM}:%{VERSION}-%{RELEASE}\\t%{ARCH}\\n";

/// The packages in `rpm -qa --queryformat` output in [`QUERY_FORMAT`], and how
/// many lines could not be read as a package.
///
/// A package is identified as RPM does, `name.arch`, so the two architectures
/// of a multilib package stay apart. `gpg-pubkey` entries, whose architecture
/// is `(none)`, are signing keys, not software, and are left out. An epoch of
/// 0 is not shown.
#[must_use]
pub fn parse_rpm_query(output: &str) -> (Vec<Package>, usize) {
    let mut packages = Vec::new();
    let mut malformed = 0;
    for line in output.lines().filter(|l| !l.trim().is_empty()) {
        let fields: Vec<&str> = line.split('\t').map(str::trim).collect();
        let [name, version, arch] = fields[..] else {
            malformed += 1;
            continue;
        };
        if name.is_empty() || arch.is_empty() {
            malformed += 1;
            continue;
        }
        if arch == "(none)" {
            continue;
        }
        let version = version.strip_prefix("0:").unwrap_or(version);
        packages.push(Package {
            manager: PackageManager::Rpm,
            id: UntrustedText::new(format!("{name}.{arch}")),
            version: Some(version)
                .filter(|v| !v.is_empty())
                .map(UntrustedText::new),
            install_root: None,
            // RPM has no "part of the base system" mark; dnf's protected
            // packages are configuration, not package metadata.
            required: false,
            // RPM does not record why a package was installed.
            explicit: None,
        });
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

    use super::{QUERY_FORMAT, parse_rpm_query};

    const PROGRAM: &str = "/usr/bin/rpm";
    const TIME_LIMIT: Duration = Duration::from_secs(60);
    const OUTPUT_LIMIT: u64 = 32 * 1024 * 1024;

    /// Installed RPM packages, source `linux.rpm`.
    #[derive(Debug, Clone, Copy, Default)]
    pub struct RpmInventory;

    /// The query's output, or `None` if it did not finish successfully within
    /// the limits.
    fn query(cancel: &CancelToken) -> Option<String> {
        let mut child = Command::new(PROGRAM)
            .args(["-qa", "--queryformat", QUERY_FORMAT])
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
        status.filter(std::process::ExitStatus::success)?;
        String::from_utf8(bytes).ok()
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
            match query(cancel) {
                None => report(SourceStatus::Partial, Vec::new()),
                Some(output) => {
                    let (packages, malformed) = parse_rpm_query(&output);
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
        let (packages, malformed) = parse_rpm_query(output);
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
    }

    #[test]
    fn empty_output_lists_nothing() {
        assert_eq!(parse_rpm_query(""), (Vec::new(), 0));
    }
}
