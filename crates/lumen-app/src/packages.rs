//! Where package managers keep their caches, asked of the tools themselves
//! (#712). The knowledge base recognizes default locations; a tool knows the
//! one it is configured with. Nothing here traverses or removes a store.

use crate::inspection::{Commands, InspectionError, Program};
use crate::tools::ToolError;
use lumen_domain::UntrustedText;
use serde::Serialize;

/// What a package manager keeps at its location.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StoreKind {
    /// Downloads the tool fetches again when needed.
    Cache,
    /// Content projects link to; the tool's prune keeps what they still use.
    SharedStore,
}

/// One tool's answer, or why there is none. Unknown is never "no cache".
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum Status {
    /// The program was not found where the user's terminal would look.
    NotInstalled,
    /// The tool could not answer completely; its location is unknown.
    Unavailable {
        /// Why, for display.
        reason: String,
    },
    /// The tool reported its version and location.
    Found {
        /// Version as the tool prints it.
        version: String,
        /// Absolute location the tool reported.
        location: String,
    },
}

/// One package manager.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Tool {
    /// Display name.
    pub name: &'static str,
    /// What its location holds.
    pub kind: StoreKind,
    /// The knowledge-base entry whose cleanup command applies (ADR-0049).
    pub entry: &'static str,
    /// That command, for display; it runs only through the confirmed flow.
    pub cleanup: Option<String>,
    /// The tool's answer.
    #[serde(flatten)]
    pub status: Status,
}

/// Every supported package manager, in a fixed order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Inventory {
    /// Version of this CLI result contract.
    pub schema_version: u32,
    /// One row per tool, installed or not.
    pub tools: Vec<Tool>,
    /// Why a location is not a cleanup promise.
    pub limitation: &'static str,
}

struct Spec {
    name: &'static str,
    program: &'static str,
    location: &'static [&'static str],
    kind: StoreKind,
    entry: &'static str,
}

const SPECS: [Spec; 3] = [
    Spec {
        name: "Homebrew",
        program: "brew",
        location: &["--cache"],
        kind: StoreKind::Cache,
        entry: "homebrew-download-cache",
    },
    Spec {
        name: "npm",
        program: "npm",
        location: &["config", "get", "cache"],
        kind: StoreKind::Cache,
        entry: "npm-cache",
    },
    Spec {
        name: "pnpm",
        program: "pnpm",
        location: &["store", "path"],
        kind: StoreKind::SharedStore,
        entry: "pnpm-store",
    },
];

/// Ask each package manager on explicit request. A missing or failing tool is
/// reported in its row; it never hides the others.
#[must_use]
pub fn inspect() -> Inventory {
    inspect_with(&mut |program| {
        Program::find(program).map(|program| Box::new(program) as Box<dyn Commands>)
    })
}

type Open<'a> = dyn FnMut(&str) -> Result<Box<dyn Commands>, InspectionError> + 'a;

fn inspect_with(open: &mut Open<'_>) -> Inventory {
    let tools = SPECS
        .iter()
        .map(|spec| Tool {
            name: spec.name,
            kind: spec.kind,
            entry: spec.entry,
            cleanup: crate::tools::builtin(spec.entry)
                .map(|command| format!("{} {}", command.program, command.args.join(" "))),
            status: match ask(spec, open) {
                Ok(status) => status,
                Err(InspectionError::Tool(ToolError::NotFound(_))) => Status::NotInstalled,
                Err(error) => Status::Unavailable {
                    reason: error.to_string(),
                },
            },
        })
        .collect();
    Inventory {
        schema_version: 1,
        tools,
        limitation: "Locations come from each tool's configuration. A cache is downloaded again when needed; pnpm's shared store holds packages projects link to, so only `pnpm store prune` should clear it. Lumen runs these commands only after you confirm them.",
    }
}

fn ask(spec: &Spec, open: &mut Open<'_>) -> Result<Status, InspectionError> {
    let mut commands = open(spec.program)?;
    let version = line(&commands.query(&["--version"])?, "version")?;
    if version.len() > 64 || !version.bytes().any(|b| b.is_ascii_digit()) {
        return Err(InspectionError::Format("version"));
    }
    let location = line(&commands.query(spec.location)?, "location")?;
    if location.len() > 4096 || !absolute(&location) {
        return Err(InspectionError::Format("location is not an absolute path"));
    }
    Ok(Status::Found {
        version: UntrustedText::new(version).display(),
        location: UntrustedText::new(location).display(),
    })
}

/// The single non-empty line a query prints, without control characters.
fn line(output: &str, what: &'static str) -> Result<String, InspectionError> {
    let mut lines = output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty());
    match (lines.next(), lines.next()) {
        (Some(line), None) if !line.chars().any(char::is_control) => Ok(line.to_owned()),
        _ => Err(InspectionError::Format(what)),
    }
}

fn absolute(path: &str) -> bool {
    let bytes = path.as_bytes();
    if cfg!(windows) {
        bytes.len() > 2
            && bytes[0].is_ascii_alphabetic()
            && bytes[1] == b':'
            && matches!(bytes[2], b'\\' | b'/')
    } else {
        bytes.first() == Some(&b'/')
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn answers(
        brew: Result<&'static str, InspectionError>,
        npm: &'static str,
        pnpm: &'static str,
    ) -> impl FnMut(&str) -> Result<Box<dyn Commands>, InspectionError> {
        let mut brew = Some(brew);
        move |program: &str| {
            let location: Result<&'static str, InspectionError> = match program {
                "brew" => brew.take().unwrap_or(Ok("")),
                "npm" => Ok(npm),
                "pnpm" => Ok(pnpm),
                _ => Err(InspectionError::Format("unreviewed program")),
            };
            let location = location?;
            Ok(Box::new(move |args: &[&str]| {
                if args == ["--version"] {
                    Ok("10.8.2\n".to_owned())
                } else {
                    Ok(location.to_owned())
                }
            }) as Box<dyn Commands>)
        }
    }

    fn statuses(inventory: &Inventory) -> Vec<&Status> {
        inventory.tools.iter().map(|tool| &tool.status).collect()
    }

    #[test]
    fn each_tool_reports_its_own_location_and_kind() {
        let inventory = inspect_with(&mut answers(
            Ok("/Users/a/Library/Caches/Homebrew\n"),
            "/Users/a/.npm\n",
            "/Users/a/Library/pnpm/store/v11\n",
        ));
        if cfg!(windows) {
            return;
        }
        assert!(
            statuses(&inventory)
                .iter()
                .all(|status| matches!(status, Status::Found { .. }))
        );
        assert_eq!(inventory.tools[2].kind, StoreKind::SharedStore);
        assert_eq!(
            inventory.tools[2].cleanup.as_deref(),
            Some("pnpm store prune")
        );
    }

    #[test]
    fn missing_failing_and_hostile_answers_are_unknown_not_empty() {
        let missing = || Err(InspectionError::Tool(ToolError::NotFound("brew".into())));
        for (npm, pnpm) in [
            ("relative/cache", "/store\n/other"),
            ("", "/store\u{1b}[2J"),
            ("/a\n\n/b", "C"),
        ] {
            let inventory = inspect_with(&mut answers(missing(), npm, pnpm));
            assert_eq!(inventory.tools[0].status, Status::NotInstalled);
            for status in &statuses(&inventory)[1..] {
                assert!(
                    matches!(status, Status::Unavailable { .. }),
                    "{npm:?} {pnpm:?}: {status:?}"
                );
            }
        }
        let failed = inspect_with(&mut answers(Err(InspectionError::Timeout), "/n", "/p"));
        assert!(matches!(failed.tools[0].status, Status::Unavailable { .. }));
    }

    #[test]
    fn an_unexpected_version_is_not_trusted() {
        let inventory = inspect_with(&mut |_: &str| {
            Ok(Box::new(|args: &[&str]| {
                Ok(if args == ["--version"] {
                    "unknown".to_owned()
                } else {
                    "/cache".to_owned()
                })
            }) as Box<dyn Commands>)
        });
        assert!(
            statuses(&inventory)
                .iter()
                .all(|status| matches!(status, Status::Unavailable { .. }))
        );
    }
}
