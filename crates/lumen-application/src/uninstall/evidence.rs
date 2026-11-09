//! Two of the gate's checks, worked out from what the caller observed
//! (ADR-0062): which helper folders a bundle has, and how many background
//! items may run it.
//!
//! Both Lumen itself and its privileged helper (ADR-0063 rule 5) gather this
//! evidence, each with its own rights, so the reasoning lives here once. The
//! filesystem is reached only through the lookups the caller passes in; this
//! module stays IO-free.

use std::collections::{BTreeMap, BTreeSet};

use lumen_domain::{AppIdScheme, Application, RawPath, SourceStatus, UntrustedText};

use super::HelperFolder;
use crate::ports::InventoryReport;
use crate::software_links::link_services;

/// What looking up one path inside a bundle found, without following links.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lookup {
    /// A directory.
    Directory,
    /// Something else: a file, a link or a special file.
    Other,
    /// Nothing by that name.
    Missing,
    /// The answer could not be read.
    Unknown,
}

/// Whether each helper folder (`Contents/Library/<name>`) exists in a bundle,
/// from `lookup`, which answers for a path given as components below the
/// bundle.
///
/// A `Contents` or `Contents/Library` that is missing means no helper folder
/// exists. One that is not a directory (a link included), or cannot be read,
/// leaves every folder unknown. A helper folder of any kind counts as present.
pub fn helper_folders(lookup: impl Fn(&[&str]) -> Lookup) -> BTreeMap<HelperFolder, Option<bool>> {
    let all = |answer| HelperFolder::ALL.map(|f| (f, answer)).into();
    for parent in [&["Contents"][..], &["Contents", "Library"]] {
        match lookup(parent) {
            Lookup::Directory => {}
            Lookup::Missing => return all(Some(false)),
            Lookup::Other | Lookup::Unknown => return all(None),
        }
    }
    HelperFolder::ALL
        .map(|folder| {
            let answer = match lookup(&["Contents", "Library", folder.name()]) {
                Lookup::Directory | Lookup::Other => Some(true),
                Lookup::Missing => Some(false),
                Lookup::Unknown => None,
            };
            (folder, answer)
        })
        .into()
}

/// The application's bundle identifier, if it has one.
pub fn bundle_id(app: &Application) -> Option<&UntrustedText> {
    app.ids
        .iter()
        .find(|id| id.scheme == AppIdScheme::BundleId)
        .map(|id| &id.value)
}

/// Programs whose job is to start other applications (`open -a X.app`).
/// Which app a background item running one starts is in its arguments, which
/// the inventory does not keep, so such an item may start any app.
pub const LAUNCHERS: [&[u8]; 1] = [b"/usr/bin/open"];

/// `path` lowercased in ASCII, followed by `/`: a prefix for anything inside
/// it, however a case-insensitive volume lets it be spelled.
fn folded_prefix(path: &[u8]) -> Vec<u8> {
    let mut prefix = path.to_ascii_lowercase();
    if !prefix.ends_with(b"/") {
        prefix.push(b'/');
    }
    prefix
}

/// How many background items may run `app`, one of `apps` (ADR-0062), or
/// `None` when that cannot be known. `resolve` answers where a path leads
/// with every link followed (`realpath`), or `None`.
///
/// The list's links ([`link_services`]) only ever miss a link, which is right
/// for display and wrong for this gate, where a missed link passes. So this
/// counts those links and, on top, any item whose program lies inside the
/// bundle ignoring letter case, whose program is a link that resolves inside
/// it, or whose label starts with the bundle ID ignoring case. An item running
/// a launcher ([`LAUNCHERS`]) may start any app, so the answer is unknown.
/// Counting too many only refuses.
pub fn linked_services(
    apps: &[Application],
    app: &Application,
    services: &InventoryReport,
    resolve: impl Fn(&RawPath) -> Option<RawPath>,
) -> Option<usize> {
    if services.status != SourceStatus::Complete {
        return None;
    }
    let all: Vec<&_> = apps.iter().collect();
    let items: Vec<&_> = services.services.iter().collect();
    let target = apps.iter().position(|a| std::ptr::eq(a, app));
    let listed: BTreeSet<usize> = link_services(&all, &items)
        .iter()
        .filter(|link| Some(link.application) == target)
        .map(|link| link.service)
        .collect();
    let bundles: Vec<Vec<u8>> = app
        .install_locations
        .iter()
        .flat_map(|path| [Some(path.clone()), resolve(path)])
        .flatten()
        .map(|path| folded_prefix(path.as_bytes()))
        .collect();
    let ids: Vec<String> = bundle_id(app)
        .map(|id| id.raw().to_ascii_lowercase())
        .filter(|id| !id.is_empty())
        .into_iter()
        .collect();
    let mut count = 0;
    for (index, service) in services.services.iter().enumerate() {
        let program = service.program.as_ref();
        if program.is_some_and(|p| LAUNCHERS.contains(&p.as_bytes())) {
            return None;
        }
        let inside = |path: &RawPath| {
            let folded = path.as_bytes().to_ascii_lowercase();
            bundles.iter().any(|bundle| folded.starts_with(bundle))
        };
        let resolved = program.and_then(&resolve);
        let label = service.label.raw().to_ascii_lowercase();
        let named = ids.iter().any(|id| {
            label
                .strip_prefix(id.as_str())
                .is_some_and(|rest| rest.is_empty() || rest.starts_with('.'))
        });
        if listed.contains(&index)
            || program.is_some_and(inside)
            || resolved.as_ref().is_some_and(inside)
            || named
        {
            count += 1;
        }
    }
    Some(count)
}

#[cfg(test)]
mod tests {
    use lumen_domain::{AppId, Service, ServiceKind, ServiceScope, SourceName};

    use super::*;

    fn raw(path: &str) -> RawPath {
        RawPath::from_unix_bytes(path.as_bytes().to_vec()).unwrap_or_else(|_| unreachable!())
    }

    fn app(path: &str, id: &str) -> Application {
        Application {
            ids: BTreeSet::from([AppId {
                scheme: AppIdScheme::BundleId,
                value: UntrustedText::new(id.to_owned()),
            }]),
            name: None,
            version: None,
            vendor: None,
            signature: None,
            install_locations: vec![raw(path)],
            sources: BTreeSet::new(),
            uninstall: None,
        }
    }

    fn service(label: &str, program: Option<&str>) -> Service {
        Service {
            kind: ServiceKind::LaunchAgent,
            scope: ServiceScope::User,
            label: UntrustedText::new(label.to_owned()),
            definition: None,
            program: program.map(raw),
            enabled: None,
            signature: None,
        }
    }

    fn report(services: Vec<Service>, status: SourceStatus) -> InventoryReport {
        InventoryReport {
            source: SourceName::new("test").unwrap_or_else(|_| unreachable!()),
            status,
            applications: Vec::new(),
            services,
            packages: Vec::new(),
        }
    }

    #[test]
    fn helper_folders_follow_the_lookup() {
        let none = helper_folders(|_| Lookup::Missing);
        assert!(none.values().all(|v| *v == Some(false)));
        let linked = helper_folders(|parts| match parts {
            ["Contents"] => Lookup::Other,
            _ => Lookup::Directory,
        });
        assert!(linked.values().all(Option::is_none));
        let one = helper_folders(|parts| match parts {
            ["Contents"] | ["Contents", "Library"] => Lookup::Directory,
            [.., "LaunchDaemons"] => Lookup::Other,
            [.., "LoginItems"] => Lookup::Unknown,
            _ => Lookup::Missing,
        });
        assert_eq!(one.get(&HelperFolder::LaunchDaemons), Some(&Some(true)));
        assert_eq!(one.get(&HelperFolder::LoginItems), Some(&None));
        assert_eq!(one.get(&HelperFolder::LaunchAgents), Some(&Some(false)));
        assert_eq!(one.len(), HelperFolder::ALL.len());
    }

    #[test]
    fn every_way_a_service_may_run_the_app_counts() {
        let apps = [app("/Applications/Zed.app", "dev.zed.Zed")];
        let count = |services: Vec<Service>| {
            linked_services(
                &apps,
                &apps[0],
                &report(services, SourceStatus::Complete),
                |path| (path == &raw("/Users/a/zed")).then(|| raw("/Applications/Zed.app/z")),
            )
        };
        assert_eq!(count(vec![service("other", Some("/bin/x"))]), Some(0));
        for one in [
            service("x", Some("/APPLICATIONS/zed.app/Contents/MacOS/z")),
            service("x", Some("/Users/a/zed")),
            service("DEV.ZED.ZED.helper", None),
            service("dev.zed.Zed", None),
        ] {
            assert_eq!(count(vec![one.clone()]), Some(1), "{one:?}");
        }
        assert_eq!(count(vec![service("dev.zed.Zedx", None)]), Some(0));
        assert_eq!(count(vec![service("x", Some("/usr/bin/open"))]), None);
        let partial = report(Vec::new(), SourceStatus::Partial);
        assert_eq!(linked_services(&apps, &apps[0], &partial, |_| None), None);
    }
}
