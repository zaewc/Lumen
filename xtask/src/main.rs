//! Repository automation. Run through the cargo alias: `cargo xtask <command>`.
//!
//! Commands:
//!
//! - `schema`: write JSON Schemas for Lumen's persisted and AI-facing records to
//!   `schemas/` (ADR-0011).
//! - `schema --check`: regenerate in memory and fail if the committed files differ,
//!   so a contract change cannot merge without a reviewed schema diff.

// A command-line tool reports to the terminal by design.
#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use lumen_domain as d;
use schemars::{JsonSchema, schema_for};

/// One exported schema: file stem and generator.
struct Export {
    stem: &'static str,
    generate: fn() -> schemars::Schema,
}

const fn export<T: JsonSchema>(stem: &'static str) -> Export {
    Export {
        stem,
        generate: || schema_for!(T),
    }
}

/// Top-level records written to disk or exchanged with other components.
/// Nested types are included through `$defs`.
const EXPORTS: &[Export] = &[
    export::<d::Evidence>("evidence"),
    export::<d::Relationship>("relationship"),
    export::<d::FilesystemEntry>("filesystem-entry"),
    export::<d::CoverageReport>("coverage-report"),
    export::<d::PolicyDecision>("policy-decision"),
    export::<d::CleanupCandidate>("cleanup-candidate"),
    export::<d::QuarantineState>("quarantine-state"),
    export::<d::ScanState>("scan-state"),
    export::<d::PlanState>("plan-state"),
    export::<d::PlatformCapabilities>("platform-capabilities"),
    export::<d::Judgment>("jev-judgment"),
    export::<d::JevTrace>("jev-trace"),
];

fn schemas_dir() -> PathBuf {
    // xtask/ lives directly under the repository root.
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("schemas")
}

fn render(export: &Export) -> Result<String, Box<dyn Error>> {
    let mut text = serde_json::to_string_pretty(&(export.generate)())?;
    text.push('\n');
    Ok(text)
}

/// Writes all schemas, or (with `check`) reports files that are missing, stale or
/// unexpected. Returns the number of problems found in check mode.
fn schema(check: bool) -> Result<usize, Box<dyn Error>> {
    let dir = schemas_dir();
    let mut problems = 0;
    let mut expected = Vec::new();
    for export in EXPORTS {
        let file = format!("{}.schema.json", export.stem);
        let path = dir.join(&file);
        let text = render(export)?;
        expected.push(file.clone());
        if check {
            match fs::read_to_string(&path) {
                Ok(current) if current == text => {}
                Ok(_) => {
                    eprintln!("schemas/{file} is out of date");
                    problems += 1;
                }
                Err(_) => {
                    eprintln!("schemas/{file} is missing");
                    problems += 1;
                }
            }
        } else {
            fs::create_dir_all(&dir)?;
            fs::write(&path, text)?;
            println!("wrote schemas/{file}");
        }
    }
    if check && dir.is_dir() {
        for entry in fs::read_dir(&dir)? {
            let name = entry?.file_name().to_string_lossy().into_owned();
            if name.ends_with(".schema.json") && !expected.contains(&name) {
                eprintln!("schemas/{name} is not produced by xtask; remove it or add an export");
                problems += 1;
            }
        }
    }
    Ok(problems)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        ["schema"] => schema(false),
        ["schema", "--check"] => schema(true),
        _ => {
            eprintln!("usage: cargo xtask schema [--check]");
            return ExitCode::from(2);
        }
    };
    match result {
        Ok(0) => ExitCode::SUCCESS,
        Ok(problems) => {
            eprintln!(
                "{problems} schema file(s) differ; run `cargo xtask schema` and commit the result"
            );
            ExitCode::FAILURE
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}
