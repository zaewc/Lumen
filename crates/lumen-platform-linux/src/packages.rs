//! Installed packages from the package managers' own databases (#806,
//! ADR-0060): dpkg's status file and pacman's local database, read as files.
//! Nothing is run and nothing is written.
//!
//! The parsers are plain functions, tested on every platform; on Linux,
//! [`DpkgInventory`] and [`PacmanInventory`] read the default locations. A
//! manager whose database is missing reports
//! [`SourceStatus::Unavailable`](lumen_domain::SourceStatus): it is not
//! installed, or keeps its database elsewhere. A record that cannot be parsed
//! makes the report partial rather than being guessed at.

use lumen_domain::{Package, PackageManager, RelationMethod, UntrustedText};

use crate::relations::{Declared, link};

/// The `name: value` fields of one control paragraph. Continuation lines
/// (starting with a space or tab) belong to the previous field and are not
/// needed here, so they are skipped.
fn fields(paragraph: &str) -> impl Iterator<Item = (&str, &str)> {
    paragraph
        .lines()
        .filter(|line| !line.starts_with([' ', '\t']))
        .filter_map(|line| line.split_once(':'))
        .map(|(name, value)| (name.trim(), value.trim()))
}

/// A field's value with its continuation lines joined, so a folded value
/// keeps its tail. Relationship fields are folded only in source control files
/// ([Debian Policy 7.1]), but a status file is untrusted text.
///
/// [Debian Policy 7.1]: https://www.debian.org/doc/debian-policy/ch-relationships.html#syntax-of-relationship-fields
fn folded(paragraph: &str, name: &str) -> Option<String> {
    let mut lines = paragraph.lines();
    while let Some(line) = lines.next() {
        let Some((field, value)) = line.split_once(':') else {
            continue;
        };
        if line.starts_with([' ', '\t']) || field.trim() != name {
            continue;
        }
        let mut joined = value.trim().to_owned();
        for next in lines.take_while(|l| l.starts_with([' ', '\t'])) {
            joined.push(' ');
            joined.push_str(next.trim());
        }
        return Some(joined);
    }
    None
}

/// Whether `name` is a package name as Debian Policy 5.6.1 allows: lower-case
/// letters, digits, `+`, `-` and `.`, at least two long, starting with a
/// letter or digit.
fn is_package_name(name: &str) -> bool {
    name.len() >= 2
        && name.starts_with(|c: char| c.is_ascii_lowercase() || c.is_ascii_digit())
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"+-.".contains(&b))
}

/// The package name of one relation, `name[:arch] [(op version)]`
/// ([deb-control(5)]), or `None` if it is not that shape. The architecture
/// qualifier is dropped: every installed package of the name is a candidate,
/// which can only make a need more ambiguous, never pin it to the wrong one.
///
/// [deb-control(5)]: https://manpages.debian.org/unstable/dpkg-dev/deb-control.5.en.html
fn relation_name(text: &str) -> Option<String> {
    relation(text, false)
}

/// Validate a relation before dropping its version. `Provides` permits only
/// equality. A version constraint is not evaluated against installed versions.
fn relation(text: &str, provides: bool) -> Option<String> {
    let text = text.trim();
    let (head, rest) = text.split_at(
        text.find(|c: char| c.is_whitespace() || c == '(')
            .unwrap_or(text.len()),
    );
    let rest = rest.trim();
    if !rest.is_empty() {
        let clause = rest.strip_prefix('(')?.strip_suffix(')')?.trim();
        let operators: &[&str] = if provides {
            &["="]
        } else {
            &["<<", "<=", "=", ">=", ">>"]
        };
        let version = operators
            .iter()
            .find_map(|op| clause.strip_prefix(op))?
            .trim();
        if !is_debian_version(version) {
            return None;
        }
    }
    let (name, arch) = head.split_once(':').unwrap_or((head, "any"));
    let arch_ok = !arch.is_empty()
        && arch
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-');
    (arch_ok && is_package_name(name)).then(|| name.to_owned())
}

/// Debian Policy 5.6.12: optional numeric epoch, upstream version, optional
/// Debian revision. Invalid syntax makes the record unknown, never empty.
fn is_debian_version(version: &str) -> bool {
    let upstream = if let Some((epoch, rest)) = version.split_once(':') {
        if epoch.is_empty() || !epoch.bytes().all(|b| b.is_ascii_digit()) {
            return false;
        }
        rest
    } else {
        version
    };
    let (upstream, revision) = upstream
        .rsplit_once('-')
        .map_or((upstream, None), |(u, r)| (u, Some(r)));
    upstream.starts_with(|c: char| c.is_ascii_digit())
        && upstream
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b".+-:~".contains(&b))
        && revision.is_none_or(|r| {
            !r.is_empty()
                && r.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"+.~".contains(&b))
        })
}

/// A relationship field's needs, each as its alternatives (`a | b`); `None`
/// if any part is not a relation.
fn needs(value: &str) -> Option<Vec<Vec<String>>> {
    if value.trim().is_empty() {
        return Some(Vec::new());
    }
    value
        .split(',')
        .map(|need| need.split('|').map(relation_name).collect())
        .collect()
}

/// A package's record of needs and provided names, from its paragraph.
fn declared(paragraph: &str, name: &str) -> (Declared, bool) {
    let field = |f| folded(paragraph, f).unwrap_or_default();
    let all_needs = needs(&field("Pre-Depends")).zip(needs(&field("Depends")));
    let raw_provides = field("Provides");
    let provides = if raw_provides.is_empty() {
        Some(Vec::new())
    } else {
        raw_provides.split(',').map(|p| relation(p, true)).collect()
    };
    let read = provides.is_some();
    (
        Declared {
            name: name.to_owned(),
            // Names alone cannot prove that this package meets a versioned
            // need. Keep all candidates when any constraint was recorded.
            allow_self_satisfaction: !field("Pre-Depends").contains('(')
                && !field("Depends").contains('('),
            provides: provides.unwrap_or_default(),
            needs: all_needs.map(|(mut pre, depends)| {
                pre.extend(depends);
                pre
            }),
        },
        read,
    )
}

/// dpkg states in which a package's files are on disk. `not-installed` and
/// `config-files` (removed, configuration kept) are not installed software.
const DPKG_PRESENT: [&str; 6] = [
    "installed",
    "unpacked",
    "half-configured",
    "half-installed",
    "triggers-awaited",
    "triggers-pending",
];

/// The installed packages in dpkg's status file (`/var/lib/dpkg/status`), and
/// how many paragraphs could not be read as a package.
///
/// A package is identified as dpkg does: by name, with its architecture unless
/// it is `all`, so `libc6:amd64` and `libc6:i386` stay apart.
///
/// Its relations are what its `Pre-Depends` and `Depends` state, matched to
/// the installed packages (see [`crate::relations`]). A package whose fields
/// cannot be read has unknown relations. If any paragraph cannot be read, or
/// any package's `Provides`, a need's candidates are not known whole, so no
/// package's relations are known.
#[must_use]
pub fn parse_dpkg_status(status: &str) -> (Vec<Package>, usize) {
    let mut packages = Vec::new();
    let mut declarations = Vec::new();
    let mut provides_read = true;
    let mut malformed = 0;
    for paragraph in status.split("\n\n").filter(|p| !p.trim().is_empty()) {
        let mut name = None;
        let mut state = None;
        let mut version = None;
        let mut arch = None;
        let mut required = false;
        for (field, value) in fields(paragraph) {
            match field {
                "Package" => name = Some(value),
                // `want flag state`, e.g. `install ok installed`.
                "Status" => state = value.split_whitespace().nth(2),
                "Version" => version = Some(value),
                "Architecture" => arch = Some(value),
                "Essential" => required |= value == "yes",
                "Priority" => required |= value == "required",
                _ => {}
            }
        }
        let (Some(name), Some(state)) = (name.filter(|n| !n.is_empty()), state) else {
            malformed += 1;
            continue;
        };
        if !DPKG_PRESENT.contains(&state) {
            continue;
        }
        let id = match arch.filter(|a| !a.is_empty() && *a != "all") {
            Some(arch) => format!("{name}:{arch}"),
            None => name.to_owned(),
        };
        let (declared, read) = declared(paragraph, name);
        provides_read &= read;
        declarations.push(declared);
        packages.push(Package {
            manager: PackageManager::Dpkg,
            id: UntrustedText::new(id),
            version: version.filter(|v| !v.is_empty()).map(UntrustedText::new),
            install_root: None,
            required,
            // dpkg does not record why a package was installed; apt keeps
            // that in another file.
            explicit: None,
            relations: None,
        });
    }
    if malformed == 0 && provides_read {
        link(&mut packages, &declarations, RelationMethod::DpkgStatus);
    }
    (packages, malformed)
}

/// The package one pacman `desc` file (`/var/lib/pacman/local/<pkg>/desc`)
/// describes, or `None` if it names none.
#[must_use]
pub fn parse_pacman_desc(desc: &str) -> Option<Package> {
    let mut name = None;
    let mut version = None;
    let mut reason = None;
    let mut lines = desc.lines().map(str::trim);
    while let Some(line) = lines.next() {
        // A header is the next section, never this one's value.
        let value = || {
            lines
                .clone()
                .next()
                .filter(|v| !v.is_empty() && !is_header(v))
        };
        match line {
            "%NAME%" => name = value(),
            "%VERSION%" => version = value(),
            "%REASON%" => reason = value(),
            _ => {}
        }
    }
    Some(Package {
        manager: PackageManager::Pacman,
        id: UntrustedText::new(name?),
        version: version.map(UntrustedText::new),
        install_root: None,
        // pacman has no "part of the base system" mark.
        required: false,
        // `%REASON%` 1 means installed as a dependency; absent or 0 means
        // installed explicitly.
        explicit: Some(reason != Some("1")),
        relations: None,
    })
}

/// Whether a `desc` line is a section header, `%NAME%`.
fn is_header(line: &str) -> bool {
    line.len() > 1 && line.starts_with('%') && line.ends_with('%')
}

/// The values of one section of a pacman `desc` file: the lines after its
/// `%HEADER%` up to a blank line or the next header ([alpm-db-desc(5)]).
/// Empty if the section is absent, as pacman omits a section with no data;
/// `None` if it is present with no value, which pacman never writes.
///
/// [alpm-db-desc(5)]: https://man.archlinux.org/man/alpm-db-desc.5
fn pacman_section<'a>(desc: &'a str, header: &str) -> Option<Vec<&'a str>> {
    let mut lines = desc.lines().map(str::trim);
    if lines.by_ref().all(|l| l != header) {
        return Some(Vec::new());
    }
    let values: Vec<&str> = lines
        .take_while(|l| !(l.is_empty() || is_header(l)))
        .collect();
    (!values.is_empty()).then_some(values)
}

/// The name in one pacman relation, `name` or `name<op>version`
/// ([alpm-package-relation(7)], [alpm-comparison(7)]), or `None` if it is not
/// a name [alpm-package-name(7)] allows. A soname relation
/// (`lib:libfoo.so.1`) keeps its prefix: only a provision written the same
/// way meets it.
///
/// [alpm-package-relation(7)]: https://man.archlinux.org/man/alpm-package-relation.7.en
/// [alpm-comparison(7)]: https://man.archlinux.org/man/alpm-comparison.7.en
/// [alpm-package-name(7)]: https://man.archlinux.org/man/alpm-package-name.7.en
#[cfg(test)]
fn pacman_relation_name(value: &str) -> Option<String> {
    pacman_relation(value, false)
}

fn pacman_relation(value: &str, provides: bool) -> Option<String> {
    let split = value.find(['<', '>', '=']).unwrap_or(value.len());
    let (name, comparison) = value.split_at(split);
    if !comparison.is_empty() {
        let operators: &[&str] = if provides {
            &["="]
        } else {
            &["<=", ">=", "<", ">", "="]
        };
        let version = operators
            .iter()
            .find_map(|op| comparison.strip_prefix(op))?;
        if version.is_empty()
            || version
                .chars()
                .any(|c| c.is_whitespace() || c.is_control() || "<=>".contains(c))
        {
            return None;
        }
    }
    let allowed = !name.is_empty()
        && !name.starts_with(['-', '.'])
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"@._+-:".contains(&b));
    allowed.then(|| name.to_owned())
}

/// A package's `%DEPENDS%` (each a need with no alternatives) and
/// `%PROVIDES%`, and whether its `%PROVIDES%` was read.
fn pacman_declared(desc: &str, name: &str) -> (Declared, bool) {
    let list = |header| -> Option<Vec<String>> {
        pacman_section(desc, header)?
            .into_iter()
            .map(|value| pacman_relation(value, header == "%PROVIDES%"))
            .collect()
    };
    let provides = list("%PROVIDES%");
    let read = provides.is_some();
    let declared = Declared {
        name: name.to_owned(),
        allow_self_satisfaction: !pacman_section(desc, "%DEPENDS%")
            .unwrap_or_default()
            .iter()
            .any(|d| d.contains(['<', '>', '='])),
        provides: provides.unwrap_or_default(),
        needs: list("%DEPENDS%").map(|names| names.into_iter().map(|n| vec![n]).collect()),
    };
    (declared, read)
}

/// The packages of pacman's local database from every package's `desc` text,
/// and how many name no package.
///
/// Relations are each package's `%DEPENDS%` (`%OPTDEPENDS%` are not needed to
/// run), matched to the installed packages as dpkg's are (see
/// [`crate::relations`]). They are known only where every `desc` names a
/// package and every `%PROVIDES%` is read; a `%DEPENDS%` that cannot be read
/// leaves that package's relations unknown.
#[must_use]
pub fn parse_pacman_local(descs: &[&str]) -> (Vec<Package>, usize) {
    let mut packages = Vec::new();
    let mut declarations = Vec::new();
    let mut provides_read = true;
    for desc in descs {
        let Some(package) = parse_pacman_desc(desc) else {
            continue;
        };
        let (declared, read) = pacman_declared(desc, package.id.raw());
        provides_read &= read;
        declarations.push(declared);
        packages.push(package);
    }
    let malformed = descs.len() - packages.len();
    if malformed == 0 && provides_read {
        link(
            &mut packages,
            &declarations,
            RelationMethod::PacmanLocalDatabase,
        );
    }
    (packages, malformed)
}

#[cfg(target_os = "linux")]
pub use inventories::{DpkgInventory, PacmanInventory};

#[cfg(target_os = "linux")]
mod inventories {
    use std::io::{ErrorKind, Read as _};
    use std::path::Path;

    use lumen_application::CancelToken;
    use lumen_application::ports::{InventoryReport, InventorySource};
    use lumen_domain::{Package, SourceName, SourceStatus};

    use super::{parse_dpkg_status, parse_pacman_local};

    /// The largest database file read; a larger one is not read as a whole and
    /// the report is partial.
    const MAX_FILE: u64 = 64 * 1024 * 1024;

    fn source(name: &str) -> SourceName {
        SourceName::new(name).unwrap_or_else(|_| unreachable!("a valid literal"))
    }

    fn report(name: &str, status: SourceStatus, packages: Vec<Package>) -> InventoryReport {
        InventoryReport {
            source: source(name),
            status,
            applications: Vec::new(),
            services: Vec::new(),
            packages,
        }
    }

    /// A file's text, `Ok(None)` if it does not exist, `Err` if it cannot be
    /// read whole.
    fn read(path: &Path) -> Result<Option<String>, ()> {
        let file = match std::fs::File::open(path) {
            Ok(file) => file,
            Err(e) if e.kind() == ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err(()),
        };
        let mut text = String::new();
        let read = file
            .take(MAX_FILE + 1)
            .read_to_string(&mut text)
            .map_err(|_| ())?;
        if u64::try_from(read).unwrap_or(u64::MAX) > MAX_FILE {
            return Err(());
        }
        Ok(Some(text))
    }

    /// Packages from dpkg's status file, source `linux.dpkg`.
    #[derive(Debug, Clone, Copy, Default)]
    pub struct DpkgInventory;

    const DPKG: &str = "linux.dpkg";
    const DPKG_STATUS: &str = "/var/lib/dpkg/status";

    impl InventorySource for DpkgInventory {
        fn name(&self) -> SourceName {
            source(DPKG)
        }

        fn collect(&self, _cancel: &CancelToken) -> InventoryReport {
            match read(Path::new(DPKG_STATUS)) {
                Ok(None) => report(DPKG, SourceStatus::Unavailable, Vec::new()),
                Err(()) => report(DPKG, SourceStatus::Partial, Vec::new()),
                Ok(Some(text)) => {
                    let (packages, malformed) = parse_dpkg_status(&text);
                    let status = if malformed == 0 {
                        SourceStatus::Complete
                    } else {
                        SourceStatus::Partial
                    };
                    report(DPKG, status, packages)
                }
            }
        }
    }

    /// Packages from pacman's local database, source `linux.pacman`.
    #[derive(Debug, Clone, Copy, Default)]
    pub struct PacmanInventory;

    const PACMAN: &str = "linux.pacman";
    const PACMAN_LOCAL: &str = "/var/lib/pacman/local";

    impl InventorySource for PacmanInventory {
        fn name(&self) -> SourceName {
            source(PACMAN)
        }

        fn collect(&self, cancel: &CancelToken) -> InventoryReport {
            let entries = match std::fs::read_dir(PACMAN_LOCAL) {
                Ok(entries) => entries,
                Err(e) if e.kind() == ErrorKind::NotFound => {
                    return report(PACMAN, SourceStatus::Unavailable, Vec::new());
                }
                Err(_) => return report(PACMAN, SourceStatus::Partial, Vec::new()),
            };
            let mut descs = Vec::new();
            let mut total_bytes = 0u64;
            let mut complete = true;
            for entry in entries {
                if cancel.is_cancelled() {
                    complete = false;
                    break;
                }
                let Ok(entry) = entry else {
                    complete = false;
                    continue;
                };
                // `ALPM_DB_VERSION` is a file beside the package directories.
                match entry.file_type() {
                    Ok(kind) if kind.is_dir() => {}
                    Ok(_) => continue,
                    Err(_) => {
                        complete = false;
                        continue;
                    }
                }
                match read(&entry.path().join("desc")) {
                    Ok(Some(desc)) => {
                        total_bytes = total_bytes
                            .saturating_add(u64::try_from(desc.len()).unwrap_or(u64::MAX));
                        if total_bytes > MAX_FILE {
                            complete = false;
                            break;
                        }
                        descs.push(desc);
                    }
                    _ => complete = false,
                }
            }
            let descs: Vec<&str> = descs.iter().map(String::as_str).collect();
            let (mut packages, malformed) = parse_pacman_local(&descs);
            complete &= malformed == 0;
            if !complete {
                // A need's candidates are not known whole.
                for package in &mut packages {
                    package.relations = None;
                }
            }
            let status = if complete {
                SourceStatus::Complete
            } else {
                SourceStatus::Partial
            };
            report(PACMAN, status, packages)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const STATUS: &str = "\
Package: libc6
Status: install ok installed
Priority: required
Architecture: amd64
Version: 2.39-0ubuntu8
Description: GNU C Library
 continuation: not a field

Package: base-files
Essential: yes
Status: install ok installed
Architecture: amd64
Version: 13ubuntu10

Package: libc6
Status: install ok installed
Architecture: i386
Version: 2.39-0ubuntu8

Package: tzdata
Status: install ok installed
Architecture: all
Version: 2024a-1

Package: old-tool
Status: deinstall ok config-files
Architecture: amd64
Version: 1.0

Package: half
Status: install reinstreq half-installed
Architecture: amd64

Status: install ok installed
Version: 9

Package: nostatus
Version: 1
";

    fn summary(packages: &[Package]) -> Vec<(String, Option<String>, bool)> {
        packages
            .iter()
            .map(|p| {
                (
                    p.id.raw().to_owned(),
                    p.version.as_ref().map(|v| v.raw().to_owned()),
                    p.required,
                )
            })
            .collect()
    }

    #[test]
    fn dpkg_lists_present_packages_by_name_and_architecture() {
        let (packages, malformed) = parse_dpkg_status(STATUS);
        assert_eq!(
            summary(&packages),
            [
                ("libc6:amd64".into(), Some("2.39-0ubuntu8".into()), true),
                ("base-files:amd64".into(), Some("13ubuntu10".into()), true),
                ("libc6:i386".into(), Some("2.39-0ubuntu8".into()), false),
                ("tzdata".into(), Some("2024a-1".into()), false),
                ("half:amd64".into(), None, false),
            ]
        );
        assert!(packages.iter().all(|p| p.manager == PackageManager::Dpkg));
        assert!(packages.iter().all(|p| p.explicit.is_none()));
        // One paragraph without a name, one without a status.
        assert_eq!(malformed, 2);
    }

    const LINKED: &str = "\
Package: mutt
Status: install ok installed
Architecture: amd64
Version: 2.2.12-0.1
Pre-Depends: libc6 (>= 2.34)
Depends: postfix | mail-transport-agent,
 libgpgme11t64:any (>= 1.18), gone
Description: text-based mailreader
 Depends: not a field

Package: postfix
Status: install ok installed
Architecture: amd64
Provides: default-mta, mail-transport-agent (= 3.8)
Depends: libc6

Package: libc6
Status: install ok installed
Architecture: amd64

Package: libgpgme11t64
Status: install ok installed
Architecture: amd64

Package: broken
Status: install ok installed
Architecture: amd64
Depends: libc6 [amd64]

Package: removed
Status: deinstall ok config-files
Architecture: amd64
Provides: mail-transport-agent
";

    /// Each relation of a package as the identifiers it names.
    fn relations(package: &Package) -> Option<Vec<Vec<String>>> {
        use lumen_domain::{PackageRelationKind, RelationMethod};
        package.relations.as_ref().map(|relations| {
            relations
                .iter()
                .map(|r| {
                    assert_eq!(r.method, RelationMethod::DpkgStatus);
                    match &r.kind {
                        PackageRelationKind::DependsOn { package, .. } => {
                            vec![package.id.raw().to_owned()]
                        }
                        PackageRelationKind::DependsOnOneOf { packages, .. } => {
                            let mut ids: Vec<String> =
                                packages.iter().map(|p| p.id.raw().to_owned()).collect();
                            ids.insert(0, "one of".to_owned());
                            ids
                        }
                        other => unreachable!("{other:?}"),
                    }
                })
                .collect()
        })
    }

    #[test]
    fn dpkg_needs_are_read_and_alternatives_stay_ambiguous() {
        let (packages, malformed) = parse_dpkg_status(LINKED);
        assert_eq!(malformed, 0);
        let of = |id: &str| {
            packages
                .iter()
                .find(|p| p.id.raw() == id)
                .and_then(relations)
        };
        let owned = |ids: &[&str]| ids.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
        assert_eq!(
            of("mutt:amd64"),
            Some(vec![
                owned(&["libc6:amd64"]),
                // Removed packages provide nothing.
                owned(&["one of", "postfix:amd64"]),
                owned(&["libgpgme11t64:amd64"]),
                owned(&["gone"]),
            ])
        );
        assert_eq!(of("postfix:amd64"), Some(vec![owned(&["libc6:amd64"])]));
        assert_eq!(of("libc6:amd64"), Some(Vec::new()));
        assert_eq!(of("broken:amd64"), None, "not a relation: unknown");
    }

    #[test]
    fn an_unreadable_paragraph_or_provides_leaves_every_relation_unknown() {
        let (packages, malformed) = parse_dpkg_status(STATUS);
        assert_eq!(malformed, 2);
        assert!(packages.iter().all(|p| p.relations.is_none()));
        let bad_provides =
            LINKED.replace("Provides: default-mta,", "Provides: default-mta | other,");
        let (packages, malformed) = parse_dpkg_status(&bad_provides);
        assert_eq!(malformed, 0);
        assert!(packages.iter().all(|p| p.relations.is_none()));
    }

    #[test]
    fn invalid_provides_and_versioned_self_providers_do_not_hide_needs() {
        for value in ["api (>= 2)", "api ()", "api (= garbage)", "api | other"] {
            let input = format!("Package: impl\nStatus: install ok installed\nProvides: {value}\n");
            let (packages, _) = parse_dpkg_status(&input);
            assert!(packages[0].relations.is_none(), "{value}");
        }
        let input = "Package: impl\nStatus: install ok installed\nProvides: api\nDepends: api (>= 2)\n\nPackage: external\nStatus: install ok installed\nProvides: api (= 2)\n";
        let (packages, _) = parse_dpkg_status(input);
        assert_eq!(
            relations(&packages[0]),
            Some(vec![vec![
                "one of".into(),
                "impl".into(),
                "external".into()
            ]])
        );
        let (packages, _) = parse_dpkg_status(&input.replace("api (>= 2)", "api"));
        assert_eq!(relations(&packages[0]), Some(Vec::new()));
    }

    #[test]
    fn relation_names_follow_debian_policy() {
        for (text, name) in [
            ("libc6", Some("libc6")),
            (" libc6:any (>= 2.34) ", Some("libc6")),
            ("g++-14:amd64", Some("g++-14")),
            ("python3(>=3.12)", Some("python3")),
            ("Libc6", None),
            ("x", None),
            ("libc6 [amd64]", None),
            ("libc6 <!nocheck>", None),
            ("libc6:", None),
            ("libc6 (>= 1) extra", None),
            ("libc6 ()", None),
            ("libc6 (garbage)", None),
            ("libc6 (>=)", None),
            ("libc6 (== 1)", None),
            ("libc6 (>= 1 bad)", None),
            ("libc6 (= 1-)", None),
            ("libc6 (= 1:2.0-1)", Some("libc6")),
            ("", None),
        ] {
            assert_eq!(relation_name(text).as_deref(), name, "{text:?}");
        }
    }

    #[test]
    fn an_empty_status_file_lists_nothing() {
        assert_eq!(parse_dpkg_status(""), (Vec::new(), 0));
        assert_eq!(parse_dpkg_status("\n\n\n").1, 0);
    }

    #[test]
    fn pacman_records_name_version_and_why_it_was_installed() {
        let explicit = "%NAME%\nfirefox\n\n%VERSION%\n132.0-1\n\n%BASE%\nfirefox\n";
        let dependency = "%NAME%\nnss\n\n%VERSION%\n3.105-1\n\n%REASON%\n1\n";
        let found = [explicit, dependency].map(parse_pacman_desc);
        assert_eq!(
            found
                .iter()
                .flatten()
                .map(|p| (
                    p.id.raw().to_owned(),
                    p.version.as_ref().map(|v| v.raw().to_owned()),
                    p.explicit
                ))
                .collect::<Vec<_>>(),
            [
                ("firefox".into(), Some("132.0-1".into()), Some(true)),
                ("nss".into(), Some("3.105-1".into()), Some(false)),
            ]
        );
        assert!(
            found
                .iter()
                .flatten()
                .all(|p| p.manager == PackageManager::Pacman && !p.required)
        );
    }

    #[test]
    fn pacman_needs_are_read_and_provided_names_stay_ambiguous() {
        use lumen_domain::{PackageRelationKind, RelationMethod};
        type Needs<'a> = Option<Vec<(bool, Vec<&'a str>)>>;
        let firefox = "%NAME%\nfirefox\n\n%VERSION%\n132.0-1\n\n%DEPENDS%\nnss>=3.105\nsh\n\
                       lib:libdbus-1.so.3\ngone\n\n%OPTDEPENDS%\nnetworkmanager: location\n";
        let nss = "%NAME%\nnss\n%VERSION%\n3.105-1\n%REASON%\n1\n";
        let bash = "%NAME%\nbash\n\n%PROVIDES%\nsh\n\n%DEPENDS%\nglibc\n";
        let dash = "%NAME%\ndash\n\n%PROVIDES%\nsh=0.5\n";
        let dbus = "%NAME%\ndbus\n\n%PROVIDES%\nlib:libdbus-1.so.3\nlibdbus-1.so=3-64\n";
        let bad = "%NAME%\nbad\n\n%DEPENDS%\nnot a name\n";
        let (packages, malformed) = parse_pacman_local(&[firefox, nss, bash, dash, dbus, bad]);
        assert_eq!(malformed, 0);
        let needs: Vec<Needs<'_>> = packages
            .iter()
            .map(|p| {
                p.relations.as_ref().map(|relations| {
                    relations
                        .iter()
                        .map(|r| {
                            assert_eq!(r.method, RelationMethod::PacmanLocalDatabase);
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
            })
            .collect();
        assert_eq!(
            needs,
            [
                Some(vec![
                    (true, vec!["nss"]),
                    (false, vec!["bash", "dash"]),
                    (false, vec!["dbus"]),
                    (true, vec!["gone"]),
                ]),
                Some(Vec::new()),
                Some(vec![(true, vec!["glibc"])]),
                Some(Vec::new()),
                Some(Vec::new()),
                None,
            ]
        );
    }

    #[test]
    fn an_unnamed_desc_or_unread_provides_leaves_every_pacman_relation_unknown() {
        let firefox = "%NAME%\nfirefox\n\n%DEPENDS%\nnss\n";
        let nss = "%NAME%\nnss\n";
        for other in [
            "%VERSION%\n1\n",
            "%NAME%\nx\n\n%PROVIDES%\n-bad\n",
            "%NAME%\nx\n\n%PROVIDES%\nsh>=2\n",
        ] {
            let (packages, _) = parse_pacman_local(&[firefox, nss, other]);
            assert!(packages.iter().all(|p| p.relations.is_none()), "{other:?}");
        }
    }

    #[test]
    fn pacman_versioned_need_keeps_external_providers() {
        let own = "%NAME%\nimpl\n\n%PROVIDES%\napi\n\n%DEPENDS%\napi>=2\n";
        let external = "%NAME%\nexternal\n\n%PROVIDES%\napi=2\n";
        let (packages, _) = parse_pacman_local(&[own, external]);
        let relations = packages[0].relations.as_deref().unwrap_or_default();
        assert_eq!(relations.len(), 1);
        assert!(
            matches!(&relations[0].kind, lumen_domain::PackageRelationKind::DependsOnOneOf { packages, .. }
            if packages.iter().map(|p| p.id.raw()).collect::<Vec<_>>() == ["impl", "external"])
        );
    }

    #[test]
    fn pacman_relation_names_follow_alpm() {
        for (value, name) in [
            ("glibc", Some("glibc")),
            ("nss>=3.105", Some("nss")),
            ("libdbus-1.so=3-64", Some("libdbus-1.so")),
            ("lib:libdbus-1.so.3", Some("lib:libdbus-1.so.3")),
            ("qt6-base<6.9", Some("qt6-base")),
            ("python@3.12", Some("python@3.12")),
            ("-x", None),
            (".x", None),
            ("two words", None),
            (">=1", None),
            ("nss>=", None),
            ("nss==2", None),
            ("nss=>2", None),
            ("nss>=two words", None),
            ("", None),
        ] {
            assert_eq!(pacman_relation_name(value).as_deref(), name, "{value:?}");
        }
    }

    #[test]
    fn a_pacman_record_without_a_name_is_none() {
        assert_eq!(parse_pacman_desc("%VERSION%\n1.0-1\n"), None);
        assert_eq!(parse_pacman_desc("%NAME%\n\n%VERSION%\n1\n"), None);
        assert_eq!(parse_pacman_desc(""), None);
        assert_eq!(parse_pacman_desc("%NAME%\n%VERSION%\n1\n"), None);
    }

    #[test]
    fn a_missing_name_before_another_header_keeps_all_relations_unknown() {
        let app = "%NAME%\napp\n%DEPENDS%\nprovider\n";
        let provider = "%NAME%\nprovider\n%VERSION%\n1\n";
        let unread = "%NAME%\n%PROVIDES%\nprovider\n";
        let (packages, malformed) = parse_pacman_local(&[app, provider, unread]);
        assert_eq!(malformed, 1);
        assert_eq!(packages.len(), 2);
        assert_eq!(
            packages[1].version.as_ref().map(UntrustedText::raw),
            Some("1")
        );
        assert!(packages.iter().all(|p| p.relations.is_none()));
    }

    #[test]
    fn a_present_section_without_values_is_unread_not_empty() {
        let provider = "%NAME%\nprovider\n";
        let empty_depends = "%NAME%\napp\n\n%DEPENDS%\n\n%VERSION%\n1\n";
        let (packages, _) = parse_pacman_local(&[provider, empty_depends]);
        assert_eq!(packages[0].relations, Some(Vec::new()));
        assert_eq!(packages[1].relations, None, "unread, never none");
        let empty_provides = "%NAME%\napp\n\n%PROVIDES%\n";
        let (packages, _) = parse_pacman_local(&[provider, empty_provides]);
        assert!(packages.iter().all(|p| p.relations.is_none()));
    }
}

#[cfg(all(test, target_os = "linux"))]
mod linux_tests {
    use lumen_application::CancelToken;
    use lumen_application::ports::InventorySource as _;
    use lumen_domain::SourceStatus;

    use super::{DpkgInventory, PacmanInventory};

    #[test]
    fn the_runners_package_database_is_read() {
        // CI's Linux runner is Ubuntu: dpkg is there, pacman is not.
        let dpkg = DpkgInventory.collect(&CancelToken::new());
        assert_eq!(dpkg.source.as_str(), "linux.dpkg");
        assert_eq!(dpkg.status, SourceStatus::Complete);
        assert!(
            dpkg.packages
                .iter()
                .any(|p| p.id.raw().starts_with("base-files") && p.required),
            "base-files is Essential"
        );
        assert!(
            dpkg.packages
                .iter()
                .any(|p| p.id.raw().starts_with("libc6:"))
        );
        // Every package's needs are read, and some name libc6.
        assert!(dpkg.packages.iter().all(|p| p.relations.is_some()));
        assert!(dpkg.packages.iter().flat_map(|p| p.relations.iter().flatten()).any(
            |r| matches!(&r.kind, lumen_domain::PackageRelationKind::DependsOn { package, .. }
                if package.id.raw().starts_with("libc6:"))
        ));
        let pacman = PacmanInventory.collect(&CancelToken::new());
        assert_eq!(pacman.source.as_str(), "linux.pacman");
        if std::path::Path::new("/var/lib/pacman/local").exists() {
            assert_ne!(pacman.status, SourceStatus::Unavailable);
        } else {
            assert_eq!(pacman.status, SourceStatus::Unavailable);
        }
    }
}
