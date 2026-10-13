//! The rules every entry must satisfy beyond its JSON shape.

use std::collections::HashSet;

use lumen_domain::{Platform, Timestamp};

use crate::{Effort, Entry, Location, Root, SafetyClass, ToolAction};

/// A rule an entry breaks. `location` is the index in `locations`.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Problem {
    /// The id is not `[a-z][a-z0-9-]*[a-z0-9]`, 2–64 bytes.
    #[error("id must be 2-64 bytes of [a-z0-9-], starting with a letter")]
    Id,
    /// The tool name is empty, too long or has control characters.
    #[error("tool must be 1-64 printable characters")]
    Tool,
    /// No locations.
    #[error("at least one location is required")]
    NoLocations,
    /// A location names no platform.
    #[error("location {location}: at least one platform is required")]
    NoPlatforms {
        /// The location.
        location: usize,
    },
    /// A Windows known folder is used for another platform.
    #[error("location {location}: a Windows known folder applies to Windows only")]
    WindowsRoot {
        /// The location.
        location: usize,
    },
    /// An environment variable name is malformed.
    #[error("location {location}: environment variable must be [A-Z_][A-Z0-9_]*, 1-64 bytes")]
    EnvVar {
        /// The location.
        location: usize,
    },
    /// A path or marker is not a safe relative path.
    #[error(
        "location {location}: `{path}` must be a relative /-separated path without `.`, `..`, \\, : or control characters"
    )]
    Path {
        /// The location.
        location: usize,
        /// The offending path.
        path: String,
    },
    /// A path would match the root itself, or every child of it.
    #[error("location {location}: the path must name something below the root")]
    TooBroad {
        /// The location.
        location: usize,
    },
    /// A project location has no markers.
    #[error("location {location}: a project location needs at least one marker")]
    NoMarkers {
        /// The location.
        location: usize,
    },
    /// No sources, a source that is not an `https` URL, or a repeated one.
    #[error("sources must be one or more distinct https URLs: `{0}`")]
    Source(String),
    /// `last_verified` is not a `YYYY-MM-DD` date.
    #[error("last_verified must be a YYYY-MM-DD date")]
    Date,
    /// A `tool_action` entry without an action.
    #[error("a tool_action entry needs an official_action")]
    MissingAction,
    /// A `not_regenerable` entry with an action or a finite effort, or a
    /// regenerable one whose effort is `impossible`.
    #[error("not_regenerable entries, and only they, have effort `impossible` and no action")]
    NotRegenerable,
    /// The action's program or arguments are malformed.
    #[error("official_action: program must be a bare name and arguments non-empty and printable")]
    Action,
}

/// Midnight UTC of a strict `YYYY-MM-DD` date.
pub(crate) fn day(text: &str) -> Option<Timestamp> {
    let bytes = text.as_bytes();
    let shaped = bytes.len() == 10
        && bytes.iter().enumerate().all(|(i, b)| match i {
            4 | 7 => *b == b'-',
            _ => b.is_ascii_digit(),
        });
    shaped
        .then(|| format!("{text}T00:00:00Z").parse().ok())
        .flatten()
}

fn printable(text: &str) -> bool {
    !text.is_empty() && !text.chars().any(char::is_control)
}

fn is_id(id: &str) -> bool {
    let bytes = id.as_bytes();
    (2..=64).contains(&bytes.len())
        && bytes[0].is_ascii_lowercase()
        && bytes[bytes.len() - 1] != b'-'
        && bytes
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-')
}

fn is_env_var(name: &str) -> bool {
    let bytes = name.as_bytes();
    (1..=64).contains(&bytes.len())
        && !bytes[0].is_ascii_digit()
        && bytes
            .iter()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || *b == b'_')
}

/// A relative path's components, if every one is a plain name (or, with
/// `wildcards`, exactly `*`).
fn components(path: &str, wildcards: bool) -> Option<Vec<&str>> {
    let parts: Vec<&str> = path.split('/').collect();
    parts
        .iter()
        .all(|part| {
            printable(part)
                && *part != "."
                && *part != ".."
                && !part.contains(['\\', ':'])
                && (*part == "*" && wildcards || !part.contains(['*', '?']))
        })
        .then_some(parts)
}

fn location(index: usize, location: &Location) -> Result<(), Problem> {
    if location.platforms.is_empty() {
        return Err(Problem::NoPlatforms { location: index });
    }
    let windows_only = location.platforms.iter().all(|p| *p == Platform::Windows);
    match &location.root {
        Root::LocalAppData | Root::RoamingAppData if !windows_only => {
            return Err(Problem::WindowsRoot { location: index });
        }
        Root::EnvVar { name } if !is_env_var(name) => {
            return Err(Problem::EnvVar { location: index });
        }
        Root::Project if location.markers.is_empty() => {
            return Err(Problem::NoMarkers { location: index });
        }
        _ => {}
    }
    let bad_path = |path: &str| Problem::Path {
        location: index,
        path: path.to_owned(),
    };
    // An environment variable may name the artifact itself; any other root is
    // far too broad to be one artifact, and so is every child of it.
    let env_var = matches!(location.root, Root::EnvVar { .. });
    if location.path.is_empty() {
        if !env_var {
            return Err(Problem::TooBroad { location: index });
        }
    } else {
        let parts = components(&location.path, true).ok_or_else(|| bad_path(&location.path))?;
        if !env_var && parts.first() == Some(&"*") {
            return Err(Problem::TooBroad { location: index });
        }
    }
    for marker in &location.markers {
        components(marker, false).ok_or_else(|| bad_path(marker))?;
    }
    Ok(())
}

fn source(url: &str) -> bool {
    url.strip_prefix("https://").is_some_and(|rest| {
        let host = rest.split(['/', '?', '#']).next().unwrap_or_default();
        !host.is_empty() && !url.chars().any(|c| c.is_whitespace() || c.is_control())
    })
}

fn action(action: &ToolAction) -> bool {
    let program = action.program.as_bytes();
    let program_ok = (1..=64).contains(&program.len())
        && program[0].is_ascii_alphanumeric()
        && program
            .iter()
            .all(|b| b.is_ascii_alphanumeric() || b".-_+".contains(b));
    let args_ok = |args: &[String]| args.iter().all(|a| printable(a));
    program_ok
        && args_ok(&action.args)
        && action
            .dry_run_args
            .as_deref()
            .is_none_or(|args| !args.is_empty() && args_ok(args))
}

/// Checks every rule, returning the first broken one.
pub(crate) fn entry(entry: &Entry) -> Result<(), Problem> {
    if !is_id(&entry.id) {
        return Err(Problem::Id);
    }
    if !printable(&entry.tool) || entry.tool.chars().count() > 64 {
        return Err(Problem::Tool);
    }
    if entry.locations.is_empty() {
        return Err(Problem::NoLocations);
    }
    for (index, found) in entry.locations.iter().enumerate() {
        location(index, found)?;
    }
    if entry.sources.is_empty() {
        return Err(Problem::Source(String::new()));
    }
    let mut seen = HashSet::new();
    for url in &entry.sources {
        if !source(url) || !seen.insert(url) {
            return Err(Problem::Source(url.clone()));
        }
    }
    if day(&entry.last_verified).is_none() {
        return Err(Problem::Date);
    }
    let impossible = entry.regeneration.effort == Effort::Impossible;
    match entry.safety_class {
        SafetyClass::ToolAction if entry.official_action.is_none() => {
            return Err(Problem::MissingAction);
        }
        SafetyClass::NotRegenerable if entry.official_action.is_some() || !impossible => {
            return Err(Problem::NotRegenerable);
        }
        SafetyClass::SelfManaged | SafetyClass::ToolAction | SafetyClass::Regenerable
            if impossible =>
        {
            return Err(Problem::NotRegenerable);
        }
        _ => {}
    }
    if entry.official_action.as_ref().is_some_and(|a| !action(a)) {
        return Err(Problem::Action);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::CARGO;

    fn cargo() -> Entry {
        serde_json::from_str(CARGO).unwrap_or_else(|_| unreachable!())
    }

    fn check(edit: impl FnOnce(&mut Entry)) -> Result<(), Problem> {
        let mut found = cargo();
        edit(&mut found);
        entry(&found)
    }

    fn tool_action() -> ToolAction {
        ToolAction {
            program: "cargo".into(),
            args: vec!["clean".into()],
            dry_run_args: Some(vec!["clean".into(), "--dry-run".into()]),
        }
    }

    #[test]
    fn the_sample_is_valid() {
        assert_eq!(check(|_| {}), Ok(()));
    }

    #[test]
    fn ids_names_and_dates_are_strict() {
        for id in ["", "a", "Cargo", "9x", "x-", "x_y", &"x".repeat(65)] {
            assert_eq!(check(|e| e.id = id.into()), Err(Problem::Id), "{id}");
        }
        assert_eq!(check(|e| e.tool = String::new()), Err(Problem::Tool));
        assert_eq!(check(|e| e.tool = "a\nb".into()), Err(Problem::Tool));
        for date in [
            "2026-13-01",
            "2026-02-30",
            "26-10-05",
            "2026-10-5",
            "2026-10-05Z",
        ] {
            assert_eq!(
                check(|e| e.last_verified = date.into()),
                Err(Problem::Date),
                "{date}"
            );
        }
    }

    #[test]
    fn paths_stay_below_their_root() {
        for path in [
            "../x", "a/../b", "./a", "/abs", "a//b", "a/", r"a\b", "C:/x", "a*/b", "a/?",
        ] {
            assert!(
                matches!(
                    check(|e| e.locations[0].path = path.into()),
                    Err(Problem::Path { location: 0, .. })
                ),
                "{path}"
            );
        }
        assert_eq!(
            check(|e| e.locations[0].path = String::new()),
            Err(Problem::TooBroad { location: 0 }),
            "never the whole home directory"
        );
        assert_eq!(
            check(|e| e.locations[0].path = "*/cache".into()),
            Err(Problem::TooBroad { location: 0 })
        );
        assert_eq!(check(|e| e.locations[0].path = ".cache/*/x".into()), Ok(()));
        assert_eq!(
            check(|e| e.locations[1].path = String::new()),
            Ok(()),
            "an environment variable may name the artifact itself"
        );
    }

    #[test]
    fn roots_are_used_where_they_exist() {
        assert_eq!(
            check(|e| e.locations[0].root = Root::LocalAppData),
            Err(Problem::WindowsRoot { location: 0 })
        );
        assert_eq!(
            check(|e| {
                e.locations[0].root = Root::LocalAppData;
                e.locations[0].platforms = [Platform::Windows].into();
            }),
            Ok(())
        );
        assert_eq!(
            check(|e| e.locations[1].root = Root::EnvVar {
                name: "cargo_home".into()
            }),
            Err(Problem::EnvVar { location: 1 })
        );
        assert_eq!(
            check(|e| e.locations[0].root = Root::Project),
            Err(Problem::NoMarkers { location: 0 }),
            "a bare directory name is not a project"
        );
        assert_eq!(
            check(|e| {
                e.locations[0].root = Root::Project;
                e.locations[0].markers = vec!["Cargo.lock".into()];
            }),
            Ok(())
        );
        assert!(matches!(
            check(|e| e.locations[0].markers = vec!["*.lock".into()]),
            Err(Problem::Path { .. })
        ));
        assert_eq!(
            check(|e| e.locations[0].platforms.clear()),
            Err(Problem::NoPlatforms { location: 0 })
        );
        assert_eq!(check(|e| e.locations.clear()), Err(Problem::NoLocations));
    }

    #[test]
    fn sources_are_distinct_https_urls() {
        for url in [
            "http://example.com",
            "https://",
            "https:///path",
            "https://a b",
            "ftp://x",
        ] {
            assert_eq!(
                check(|e| e.sources = vec![url.into()]),
                Err(Problem::Source(url.into())),
                "{url}"
            );
        }
        assert_eq!(
            check(|e| e.sources.push(e.sources[0].clone())),
            Err(Problem::Source(cargo().sources[0].clone()))
        );
        assert_eq!(
            check(|e| e.sources.clear()),
            Err(Problem::Source(String::new()))
        );
    }

    #[test]
    fn safety_classes_match_actions_and_effort() {
        assert_eq!(
            check(|e| e.safety_class = SafetyClass::ToolAction),
            Err(Problem::MissingAction)
        );
        assert_eq!(
            check(|e| {
                e.safety_class = SafetyClass::ToolAction;
                e.official_action = Some(tool_action());
            }),
            Ok(())
        );
        assert_eq!(
            check(|e| e.safety_class = SafetyClass::NotRegenerable),
            Err(Problem::NotRegenerable),
            "effort must be impossible"
        );
        assert_eq!(
            check(|e| {
                e.safety_class = SafetyClass::NotRegenerable;
                e.regeneration.effort = Effort::Impossible;
            }),
            Ok(())
        );
        assert_eq!(
            check(|e| {
                e.safety_class = SafetyClass::NotRegenerable;
                e.regeneration.effort = Effort::Impossible;
                e.official_action = Some(tool_action());
            }),
            Err(Problem::NotRegenerable),
            "irreplaceable data gets no action"
        );
        assert_eq!(
            check(|e| e.regeneration.effort = Effort::Impossible),
            Err(Problem::NotRegenerable)
        );
    }

    #[test]
    fn actions_are_bare_programs_with_printable_arguments() {
        let with = |edit: fn(&mut ToolAction)| {
            check(|e| {
                let mut action = tool_action();
                edit(&mut action);
                e.official_action = Some(action);
            })
        };
        assert_eq!(with(|_| {}), Ok(()));
        for program in ["/usr/bin/cargo", "../cargo", r"bin\cargo", "", "-rf", "a b"] {
            assert_eq!(
                check(|e| {
                    let mut action = tool_action();
                    action.program = program.into();
                    e.official_action = Some(action);
                }),
                Err(Problem::Action),
                "{program}"
            );
        }
        assert_eq!(with(|a| a.args.push(String::new())), Err(Problem::Action));
        assert_eq!(with(|a| a.args.push("x\0".into())), Err(Problem::Action));
        assert_eq!(
            with(|a| a.dry_run_args = Some(vec![])),
            Err(Problem::Action)
        );
        assert_eq!(with(|a| a.dry_run_args = None), Ok(()));
    }
}
