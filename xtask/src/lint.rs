//! `cargo xtask lint`: structural checks that keep the repository legible to
//! people and coding agents (ADR-0056).
//!
//! - **Layering:** internal crate dependencies follow
//!   `docs/architecture/system.md` §5.1.
//! - **Purity:** `lumen-domain`, `lumen-policy` and `lumen-graph` use no IO and no
//!   platform code; `lumen-application` has no platform code.
//! - **ADR index:** every ADR is listed once in `docs/decisions/README.md`, and no
//!   two ADRs share a number.
//! - **Links:** relative Markdown links and `CLAUDE.md` `@imports` resolve.
//!
//! Every problem names the rule and the fix, so an agent can act on the message
//! alone.

use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

/// A crate's place in the layering of system.md §5.1.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Layer {
    /// `lumen-domain`: depends on no other Lumen crate.
    Domain,
    /// `lumen-policy`, `lumen-graph`: pure, on top of the domain.
    Pure,
    /// `lumen-application`: use cases and ports.
    Application,
    /// Adapters implementing ports.
    Adapter,
    /// Shared test fixtures, usable by adapters' tests and conformance features.
    TestSupport,
    /// Composition roots and tools: may depend on anything.
    Root,
}

const LAYERS: &[(&str, Layer)] = &[
    ("lumen-domain", Layer::Domain),
    ("lumen-policy", Layer::Pure),
    ("lumen-graph", Layer::Pure),
    ("lumen-application", Layer::Application),
    ("lumen-cluster", Layer::Adapter),
    ("lumen-devkb", Layer::Adapter),
    ("lumen-fs-exec", Layer::Adapter),
    ("lumen-jev", Layer::Adapter),
    ("lumen-platform-linux", Layer::Adapter),
    ("lumen-platform-macos", Layer::Adapter),
    ("lumen-platform-windows", Layer::Adapter),
    ("lumen-scan", Layer::Adapter),
    ("lumen-store-sqlite", Layer::Adapter),
    ("lumen-telemetry", Layer::Adapter),
    ("lumen-testkit", Layer::TestSupport),
    ("lumen-api", Layer::Root),
    ("lumen-app", Layer::Root),
    ("lumen-cli", Layer::Root),
    ("lumen-desktop", Layer::Root),
    ("lumen-ffi", Layer::Root),
    ("lumen-helper", Layer::Root),
    ("lumen-jev-eval", Layer::Root),
    ("xtask", Layer::Root),
];

/// Crates that must stay IO-free and platform-free, and the source patterns that
/// break that. `lumen-application` may use neither platform code nor
/// `#[cfg(target_os)]`, but its ports are allowed to name `std::io` types.
const PURE_CRATES: &[&str] = &["lumen-domain", "lumen-policy", "lumen-graph"];
const IO_PATTERNS: &[&str] = &[
    "std::fs",
    "std::net",
    "std::process",
    "std::thread",
    "std::env",
    "tokio",
];
const PLATFORM_PATTERNS: &[&str] = &["target_os", "target_family", "cfg(unix", "cfg(windows"];

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn layer_of(name: &str) -> Option<Layer> {
    LAYERS.iter().find(|(n, _)| *n == name).map(|(_, l)| *l)
}

/// Whether a crate in `from` may have a normal or build dependency on `to`, and if
/// not, what it may depend on instead.
fn allowed(from: (&str, Layer), to: (&str, Layer)) -> Result<(), &'static str> {
    use Layer::{Adapter, Application, Domain, Pure, Root, TestSupport};
    match (from.1, to.1) {
        (Root, _)
        | (Pure | Application | Adapter | TestSupport, Domain)
        | (Application | Adapter | TestSupport, Pure)
        | (Adapter | TestSupport, Application)
        | (Adapter, TestSupport) => Ok(()),
        // Platform enumerators build on the scanner (system.md §5.1).
        (Adapter, Adapter) if from.0.starts_with("lumen-platform-") && to.0 == "lumen-scan" => {
            Ok(())
        }
        (Domain, _) => Err("lumen-domain depends on no other Lumen crate"),
        (Pure, _) => Err("a pure crate may depend only on lumen-domain"),
        (Application, _) => Err(
            "lumen-application depends only on lumen-domain, lumen-policy and lumen-graph; \
             it defines ports and never depends on an adapter",
        ),
        (Adapter | TestSupport, _) => Err(
            "an adapter depends on lumen-application and the pure crates, never on another \
             adapter or a composition root",
        ),
    }
}

fn layering(root: &Path) -> Result<usize, Box<dyn Error>> {
    let output = Command::new(env!("CARGO"))
        .current_dir(root)
        .args(["metadata", "--format-version", "1", "--no-deps", "--locked"])
        .output()?;
    if !output.status.success() {
        return Err(format!(
            "cargo metadata failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    let metadata: Value = serde_json::from_slice(&output.stdout)?;
    let packages = metadata["packages"].as_array().ok_or("no packages")?;
    let mut problems = 0;
    for package in packages {
        let name = package["name"].as_str().ok_or("package without a name")?;
        let Some(from) = layer_of(name) else {
            eprintln!(
                "{name}: not in the layering table. Add it to LAYERS in xtask/src/lint.rs and \
                 to the crate diagram in docs/architecture/system.md §5.1."
            );
            problems += 1;
            continue;
        };
        for dep in package["dependencies"].as_array().into_iter().flatten() {
            let dep_name = dep["name"].as_str().unwrap_or_default();
            // Tests may use any crate; only what ships is layered.
            if dep["kind"].as_str() == Some("dev") {
                continue;
            }
            let Some(to) = layer_of(dep_name) else {
                continue;
            };
            if let Err(rule) = allowed((name, from), (dep_name, to)) {
                eprintln!(
                    "{name} depends on {dep_name}: {rule} (docs/architecture/system.md §5.1). \
                     Move the code that needs {dep_name} up into a crate allowed to use it, or \
                     define a port in lumen-application and implement it in an adapter."
                );
                problems += 1;
            }
        }
    }
    Ok(problems)
}

/// Returns the first forbidden pattern on a line of code, ignoring comments.
fn forbidden_on(line: &str, patterns: &[&'static str]) -> Option<&'static str> {
    let code = line.split("//").next().unwrap_or_default();
    patterns.iter().copied().find(|p| code.contains(p))
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), Box<dyn Error>> {
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            rust_files(&path, out)?;
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
    Ok(())
}

fn purity(root: &Path) -> Result<usize, Box<dyn Error>> {
    let mut problems = 0;
    let checks = PURE_CRATES
        .iter()
        .map(|c| (*c, [IO_PATTERNS, PLATFORM_PATTERNS].concat()))
        .chain([("lumen-application", PLATFORM_PATTERNS.to_vec())]);
    for (krate, patterns) in checks {
        let mut files = Vec::new();
        rust_files(&root.join("crates").join(krate).join("src"), &mut files)?;
        files.sort();
        for file in files {
            let text = fs::read_to_string(&file)?;
            let shown = file.strip_prefix(root).unwrap_or(&file).display();
            for (number, line) in text.lines().enumerate() {
                if let Some(pattern) = forbidden_on(line, &patterns) {
                    eprintln!(
                        "{shown}:{}: `{pattern}` in {krate}, which has no IO and no platform \
                         code (docs/architecture/system.md §5.1). Take the data as an argument \
                         and let lumen-application or an adapter fetch it through a port.",
                        number + 1
                    );
                    problems += 1;
                }
            }
        }
    }
    Ok(problems)
}

fn is_markdown(path: &Path) -> bool {
    path.extension().is_some_and(|e| e == "md")
}

fn adr_index(root: &Path) -> Result<usize, Box<dyn Error>> {
    let dir = root.join("docs/decisions");
    let index = fs::read_to_string(dir.join("README.md"))?;
    let mut names: Vec<String> = fs::read_dir(&dir)?
        .map(|e| e.map(|e| e.file_name().to_string_lossy().into_owned()))
        .collect::<Result<_, _>>()?;
    names.retain(|n| is_markdown(Path::new(n)) && n.starts_with(|c: char| c.is_ascii_digit()));
    names.sort();
    let mut problems = 0;
    let mut previous = "";
    for name in &names {
        let number = name.get(..4).unwrap_or_default();
        if number == "0000" {
            continue;
        }
        if previous.get(..4) == Some(number) {
            eprintln!(
                "docs/decisions/{previous} and {name} share ADR number {number}. Give the newer \
                 one the next free number (the last row of docs/decisions/README.md plus one)."
            );
            problems += 1;
        }
        previous = name;
        if !index.contains(&format!("]({name})")) {
            eprintln!(
                "docs/decisions/{name} is not in the ADR index. Add a row \
                 `| [{number}]({name}) | <title> | <status> |` to docs/decisions/README.md."
            );
            problems += 1;
        }
    }
    Ok(problems)
}

/// Relative link targets in Markdown text, outside fenced code blocks.
fn link_targets(text: &str) -> Vec<&str> {
    let mut targets = Vec::new();
    let mut fenced = false;
    for line in text.lines() {
        if line.trim_start().starts_with("```") {
            fenced = !fenced;
            continue;
        }
        if fenced {
            continue;
        }
        let mut rest = line;
        while let Some(start) = rest.find("](") {
            rest = &rest[start + 2..];
            let end = rest
                .find(|c: char| c == ')' || c.is_whitespace())
                .unwrap_or(rest.len());
            let target = rest[..end].split(['#', '?']).next().unwrap_or_default();
            let external = rest[..end].contains("://") || rest.starts_with("mailto:");
            if !target.is_empty() && !external && !target.contains('%') {
                targets.push(target);
            }
            rest = &rest[end..];
        }
    }
    targets
}

/// `@path` imports: lines of a `CLAUDE.md` that consist of one `@` path.
fn imports(text: &str) -> impl Iterator<Item = &str> {
    text.lines()
        .filter_map(|l| l.trim().strip_prefix('@'))
        .filter(|p| !p.is_empty() && !p.contains(char::is_whitespace))
}

const SKIP_DIRS: &[&str] = &[
    ".git",
    ".turbo",
    ".expo",
    "node_modules",
    "target",
    "dist",
    "build",
    "worktrees",
    "Pods",
];

fn markdown_files(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), Box<dyn Error>> {
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        if path.is_dir() {
            if !SKIP_DIRS.contains(&name.as_ref()) {
                markdown_files(&path, out)?;
            }
        } else if is_markdown(&path) {
            out.push(path);
        }
    }
    Ok(())
}

fn links(root: &Path) -> Result<usize, Box<dyn Error>> {
    let mut files = Vec::new();
    markdown_files(root, &mut files)?;
    files.sort();
    let mut problems = 0;
    for file in files {
        let text = fs::read_to_string(&file)?;
        let dir = file.parent().unwrap_or(root);
        let shown = file.strip_prefix(root).unwrap_or(&file).display();
        let is_claude = file.file_name().is_some_and(|n| n == "CLAUDE.md");
        let mut targets = link_targets(&text);
        if is_claude {
            targets.extend(imports(&text));
        }
        for target in targets {
            if !dir.join(target).exists() {
                eprintln!(
                    "{shown}: link to {target} does not resolve. Point it at the file's current \
                     path (relative to {shown}), or remove the link if the file is gone."
                );
                problems += 1;
            }
        }
    }
    Ok(problems)
}

/// Runs every check. Returns the number of problems found.
pub(crate) fn lint() -> Result<usize, Box<dyn Error>> {
    let root = root();
    let problems = layering(&root)? + purity(&root)? + adr_index(&root)? + links(&root)?;
    if problems == 0 {
        println!("lint: layering, purity, ADR index and links are in order");
    } else {
        eprintln!("lint: {problems} problem(s); each line above says how to fix it");
    }
    Ok(problems)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_layer_rule_of_system_md() {
        let ok = |a: &str, b: &str| {
            allowed(
                (a, layer_of(a).unwrap_or(Layer::Root)),
                (b, layer_of(b).unwrap_or(Layer::Root)),
            )
            .is_ok()
        };
        assert!(ok("lumen-policy", "lumen-domain"));
        assert!(!ok("lumen-domain", "lumen-policy"));
        assert!(!ok("lumen-policy", "lumen-graph"));
        assert!(ok("lumen-application", "lumen-graph"));
        assert!(!ok("lumen-application", "lumen-scan"));
        assert!(ok("lumen-fs-exec", "lumen-application"));
        assert!(!ok("lumen-fs-exec", "lumen-scan"));
        assert!(ok("lumen-platform-macos", "lumen-scan"));
        assert!(ok("lumen-scan", "lumen-testkit"));
        assert!(!ok("lumen-jev", "lumen-app"));
        assert!(ok("lumen-app", "lumen-fs-exec"));
    }

    #[test]
    fn comments_do_not_count_as_code() {
        assert_eq!(
            forbidden_on("//! no `#[cfg(target_os)]`;", PLATFORM_PATTERNS),
            None
        );
        assert_eq!(
            forbidden_on("use std::fs::File; // reads", IO_PATTERNS),
            Some("std::fs")
        );
    }

    #[test]
    fn links_skip_code_blocks_anchors_and_urls() {
        let text = "[a](a.md#x) [b](https://x.y/z) [c](#top)\n```\n[d](d.md)\n```\n[e](../e.md)";
        assert_eq!(link_targets(text), ["a.md", "../e.md"]);
        assert_eq!(
            imports("# x\n\n@AGENTS.md\n").collect::<Vec<_>>(),
            ["AGENTS.md"]
        );
    }
}
