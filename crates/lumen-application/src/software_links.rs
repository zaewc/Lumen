//! Which installed application a background item belongs to (ADR-0060 step 2),
//! decided only from evidence a reader can check. A matching name is never
//! enough, and unknown ownership is never reported as confirmed.
//!
//! - [`LinkMethod::ProgramInsideApplication`] (verified): the program the item
//!   runs lies inside a location the application's inventory recorded,
//!   compared component by component, never as a string prefix, so
//!   `/Applications/Foo.app.old/x` is not inside `/Applications/Foo.app`. Unix
//!   components compare byte for byte; Windows components ignore ASCII case,
//!   as Windows does, and otherwise compare exactly, so a difference Windows
//!   might ignore only ever loses a link. Paths with `.` or `..` components are
//!   not compared, and a location fewer than two components below its root
//!   (`C:\Tools`, `/Applications`) is too broad to prove anything.
//! - [`LinkMethod::LabelMatchesBundleId`] (heuristic, never verified): a
//!   launchd label equals a bundle identifier or starts with it and a dot. It
//!   is tried only for an item whose program lies inside no application.
//!
//! When an item matches more than one application (nested installs, one
//! folder shared by a suite), none is chosen: it is linked to every match,
//! each marked ambiguous and none verified. Records that share an identifier
//! are one application. An item with no program, or a relative one, links to
//! nothing by its program. Pure and IO-free.

use lumen_domain::{AppIdScheme, Application, PathFlavor, RawPath, Service, ServiceKind};

/// The evidence a link rests on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum LinkMethod {
    /// The item's program lies inside the application's recorded location.
    ProgramInsideApplication,
    /// The launchd label equals or extends the bundle identifier: a naming
    /// convention, not proof.
    LabelMatchesBundleId,
}

/// One background item linked to one application.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ServiceLink {
    /// Index into the services given.
    pub service: usize,
    /// Index into the applications given.
    pub application: usize,
    /// The evidence.
    pub method: LinkMethod,
    /// The item matched other applications too; this is one candidate.
    pub ambiguous: bool,
}

impl ServiceLink {
    /// Whether the link is proven: the program lies inside this application
    /// and no other.
    pub const fn verified(&self) -> bool {
        matches!(self.method, LinkMethod::ProgramInsideApplication) && !self.ambiguous
    }
}

/// Links each service to the applications it belongs to, ordered by service
/// and then application.
pub fn link_services(applications: &[&Application], services: &[&Service]) -> Vec<ServiceLink> {
    let locations: Vec<Vec<Components>> = applications
        .iter()
        .map(|app| {
            app.install_locations
                .iter()
                .filter_map(components)
                .filter(|c| c.depth() >= 2)
                .collect()
        })
        .collect();
    let mut links = Vec::new();
    for (service, item) in services.iter().enumerate() {
        let program = item.program.as_ref().and_then(components);
        let mut method = LinkMethod::ProgramInsideApplication;
        let mut matches: Vec<usize> = program.as_ref().map_or_else(Vec::new, |program| {
            (0..applications.len())
                .filter(|&a| locations[a].iter().any(|l| program.is_inside(l)))
                .collect()
        });
        if matches.is_empty() && launchd(item.kind) {
            method = LinkMethod::LabelMatchesBundleId;
            let label = item.label.raw();
            matches = (0..applications.len())
                .filter(|&a| label_matches(label, applications[a]))
                .collect();
        }
        let ambiguous = matches.iter().enumerate().any(|(i, &a)| {
            matches[i + 1..]
                .iter()
                .any(|&b| applications[a].ids.is_disjoint(&applications[b].ids))
        });
        links.extend(matches.into_iter().map(|application| ServiceLink {
            service,
            application,
            method,
            ambiguous,
        }));
    }
    links
}

const fn launchd(kind: ServiceKind) -> bool {
    matches!(kind, ServiceKind::LaunchAgent | ServiceKind::LaunchDaemon)
}

fn label_matches(label: &str, app: &Application) -> bool {
    app.ids
        .iter()
        .filter(|id| id.scheme == AppIdScheme::BundleId)
        .map(|id| id.value.raw())
        .filter(|bundle| !bundle.is_empty())
        .any(|bundle| {
            label
                .strip_prefix(bundle)
                .is_some_and(|rest| rest.is_empty() || rest.starts_with('.'))
        })
}

/// An absolute path as its root and the components below it.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Components<'a> {
    flavor: PathFlavor,
    root: Vec<&'a [u8]>,
    below: Vec<&'a [u8]>,
}

impl Components<'_> {
    pub(crate) fn depth(&self) -> usize {
        self.below.len()
    }

    /// The same location as `other`, compared component by component.
    pub(crate) fn is_same(&self, other: &Components<'_>) -> bool {
        self.depth() == other.depth() && self.is_at_or_inside(other)
    }

    /// Strictly below `location`.
    fn is_inside(&self, location: &Components<'_>) -> bool {
        self.depth() > location.depth() && self.is_at_or_inside(location)
    }

    fn is_at_or_inside(&self, location: &Components<'_>) -> bool {
        let same = |a: &[u8], b: &[u8]| match self.flavor {
            PathFlavor::Unix => a == b,
            // WTF-8 never puts an ASCII byte inside a longer sequence.
            PathFlavor::Windows => a.eq_ignore_ascii_case(b),
        };
        let all_same = |a: &[&[u8]], b: &[&[u8]]| {
            a.len() == b.len() && a.iter().zip(b).all(|(x, y)| same(x, y))
        };
        self.flavor == location.flavor
            && all_same(&self.root, &location.root)
            && self.below.len() >= location.below.len()
            && all_same(&self.below[..location.below.len()], &location.below)
    }
}

/// Splits an absolute path; `None` for a relative path or one with `.` or `..`.
pub(crate) fn components(path: &RawPath) -> Option<Components<'_>> {
    let bytes = path.as_bytes();
    let (root, rest): (Vec<&[u8]>, &[u8]) = match path.flavor() {
        PathFlavor::Unix => (Vec::new(), bytes.strip_prefix(b"/")?),
        PathFlavor::Windows => {
            let separator = |b: &u8| *b == b'\\' || *b == b'/';
            match bytes {
                // `\\server\share\…`: the server and share are the root.
                [a, b, rest @ ..] if separator(a) && separator(b) => {
                    let mut parts = rest.split(separator);
                    let server = parts.next().filter(|p| !p.is_empty())?;
                    let share = parts.next().filter(|p| !p.is_empty())?;
                    let used = server.len() + share.len() + 1;
                    (vec![b"\\\\".as_slice(), server, share], rest.get(used..)?)
                }
                // `X:\…`
                [letter, b':', s, ..] if letter.is_ascii_alphabetic() && separator(s) => {
                    (vec![bytes.get(..2)?], bytes.get(3..)?)
                }
                _ => return None,
            }
        }
    };
    let below: Vec<&[u8]> = match path.flavor() {
        PathFlavor::Unix => rest.split(|b| *b == b'/').collect(),
        PathFlavor::Windows => rest.split(|b| *b == b'\\' || *b == b'/').collect(),
    };
    let below: Vec<&[u8]> = below.into_iter().filter(|c| !c.is_empty()).collect();
    if below.iter().any(|c| *c == b"." || *c == b"..") {
        return None;
    }
    Some(Components {
        flavor: path.flavor(),
        root,
        below,
    })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use lumen_domain::{AppId, ServiceScope, UntrustedText};

    use super::*;

    fn unix(path: &str) -> RawPath {
        RawPath::from_unix_bytes(path.as_bytes().to_vec()).unwrap_or_else(|_| unreachable!())
    }

    fn windows(path: &str) -> RawPath {
        let units: Vec<u16> = path.encode_utf16().collect();
        RawPath::from_windows_wide(&units).unwrap_or_else(|_| unreachable!())
    }

    fn app(id: &str, location: RawPath) -> Application {
        Application {
            ids: BTreeSet::from([AppId {
                scheme: AppIdScheme::BundleId,
                value: UntrustedText::new(id),
            }]),
            name: None,
            version: None,
            vendor: None,
            signature: None,
            install_locations: vec![location],
            sources: BTreeSet::new(),
            uninstall: None,
        }
    }

    fn service(kind: ServiceKind, label: &str, program: Option<RawPath>) -> Service {
        Service {
            kind,
            scope: ServiceScope::User,
            label: UntrustedText::new(label),
            definition: None,
            program,
            enabled: None,
            signature: None,
        }
    }

    fn agent(label: &str, program: &str) -> Service {
        service(ServiceKind::LaunchAgent, label, Some(unix(program)))
    }

    fn links(apps: &[Application], services: &[Service]) -> Vec<(usize, usize, LinkMethod, bool)> {
        let apps: Vec<&Application> = apps.iter().collect();
        let services: Vec<&Service> = services.iter().collect();
        link_services(&apps, &services)
            .into_iter()
            .map(|l| (l.service, l.application, l.method, l.verified()))
            .collect()
    }

    const INSIDE: LinkMethod = LinkMethod::ProgramInsideApplication;
    const LABEL: LinkMethod = LinkMethod::LabelMatchesBundleId;

    #[test]
    fn a_program_inside_one_application_is_verified() {
        let apps = [app("com.example.Foo", unix("/Applications/Foo.app"))];
        let services = [agent(
            "org.unrelated.updater",
            "/Applications/Foo.app/Contents/MacOS/updater",
        )];
        assert_eq!(links(&apps, &services), [(0, 0, INSIDE, true)]);
    }

    #[test]
    fn a_look_alike_prefix_is_not_inside() {
        let apps = [app("com.example.Foo", unix("/Applications/Foo.app"))];
        let services = [
            agent("x.a", "/Applications/Foo.app.old/Contents/MacOS/foo"),
            agent("x.b", "/Applications/Foo.application"),
            agent("x.c", "/Applications/Foo.app/../Bar.app/Contents/MacOS/bar"),
            agent("x.d", "/Applications/Foo.app"),
        ];
        assert_eq!(links(&apps, &services), []);
    }

    #[test]
    fn nested_locations_are_ambiguous_and_never_verified() {
        let apps = [
            app("com.example.Foo", unix("/Applications/Foo.app")),
            app(
                "com.example.Helper",
                unix("/Applications/Foo.app/Contents/Helpers/Helper.app"),
            ),
        ];
        let services = [agent(
            "x.helper",
            "/Applications/Foo.app/Contents/Helpers/Helper.app/Contents/MacOS/helper",
        )];
        let found: Vec<_> = {
            let apps: Vec<&Application> = apps.iter().collect();
            link_services(&apps, &[&services[0]])
        };
        assert_eq!(found.len(), 2, "linked to every candidate");
        assert!(found.iter().all(|l| l.ambiguous && !l.verified()));
    }

    #[test]
    fn two_records_of_one_application_are_not_ambiguous() {
        let apps = [
            app("com.example.Foo", unix("/Applications/Foo.app")),
            app("com.example.Foo", unix("/Applications/Foo.app")),
        ];
        let services = [agent("x", "/Applications/Foo.app/Contents/MacOS/foo")];
        assert_eq!(
            links(&apps, &services),
            [(0, 0, INSIDE, true), (0, 1, INSIDE, true)]
        );
    }

    #[test]
    fn a_label_match_alone_is_heuristic() {
        let apps = [app("com.example.Foo", unix("/Applications/Foo.app"))];
        let services = [
            agent("com.example.Foo.helper", "/Library/Foo/helper"),
            agent("com.example.Foo", "/Library/Foo/agent"),
            agent("com.example.Foobar", "/Library/Foobar/agent"),
            service(ServiceKind::LaunchDaemon, "com.example.Foo.d", None),
            service(ServiceKind::WindowsService, "com.example.Foo", None),
        ];
        assert_eq!(
            links(&apps, &services),
            [
                (0, 0, LABEL, false),
                (1, 0, LABEL, false),
                (3, 0, LABEL, false)
            ]
        );
    }

    #[test]
    fn unrelated_relative_or_programless_services_link_nothing() {
        let apps = [app("com.example.Foo", unix("/Applications/Foo.app"))];
        let services = [
            agent("org.other.agent", "/usr/local/bin/other"),
            agent("org.other.rel", "Applications/Foo.app/Contents/MacOS/foo"),
            service(ServiceKind::LaunchAgent, "org.other.none", None),
        ];
        assert_eq!(links(&apps, &services), []);
    }

    #[test]
    fn broad_locations_prove_nothing() {
        let apps = [
            app("a", unix("/Applications")),
            app("b", windows(r"C:\Tools")),
            app("c", windows(r"C:\")),
        ];
        let services = [
            agent("x", "/Applications/Foo.app/Contents/MacOS/foo"),
            service(
                ServiceKind::WindowsService,
                "y",
                Some(windows(r"C:\Tools\agent.exe")),
            ),
        ];
        assert_eq!(links(&apps, &services), []);
    }

    #[test]
    fn windows_components_ignore_ascii_case_only() {
        let apps = [
            app("a", windows(r"C:\Program Files\Example")),
            app("b", windows(r"\\server\share\Apps\Tool")),
            app("c", windows(r"C:\Program Files\Ünïcode")),
        ];
        let services = [
            service(
                ServiceKind::WindowsService,
                "svc",
                Some(windows(r"c:\PROGRAM FILES\example\bin/svc.exe")),
            ),
            service(
                ServiceKind::StartupEntry,
                "run",
                Some(windows(r"\\SERVER\Share\apps\tool\run.exe")),
            ),
            service(
                ServiceKind::StartupEntry,
                "uni",
                Some(windows(r"C:\Program Files\üNÏCODE\x.exe")),
            ),
            service(
                ServiceKind::StartupEntry,
                "unix",
                Some(unix("/Program Files/Example/x.exe")),
            ),
        ];
        assert_eq!(
            links(&apps, &services),
            [(0, 0, INSIDE, true), (1, 1, INSIDE, true)],
            "a non-ASCII case difference loses the link; flavors never mix"
        );
    }
}
