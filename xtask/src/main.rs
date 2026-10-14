//! Repository automation. Run through the cargo alias: `cargo xtask <command>`.
//!
//! Commands:
//!
//! - `schema`: write JSON Schemas for Lumen's persisted and AI-facing records to
//!   `schemas/` (ADR-0011).
//! - `schema --check`: regenerate in memory and fail if the committed files differ,
//!   so a contract change cannot merge without a reviewed schema diff.
//! - `devkb --max-age-days <N>`: fail if a knowledge-base entry was last verified
//!   more than `N` days ago, or on a future date (ADR-0021). Run on a schedule, not
//!   on pull requests, so time passing never fails unrelated changes.

// A command-line tool reports to the terminal by design.
#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

use lumen_devkb::{DateIssue, KnowledgeBase};
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
    export::<d::JevRequest>("jev-request"),
    export::<d::Judgment>("jev-judgment"),
    export::<d::JevTrace>("jev-trace"),
    export::<lumen_devkb::Entry>("devkb-entry"),
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
    if problems > 0 {
        eprintln!(
            "{problems} schema file(s) differ; run `cargo xtask schema` and commit the result"
        );
    }
    Ok(problems)
}

/// Reports knowledge-base entries whose `last_verified` needs attention. Returns
/// the number found.
fn devkb(max_age_days: &str) -> Result<usize, Box<dyn Error>> {
    let max_age_days: i64 = max_age_days.parse()?;
    let since_epoch = SystemTime::now().duration_since(UNIX_EPOCH)?;
    let today = d::Timestamp::from_unix(i64::try_from(since_epoch.as_secs())?, 0)?;
    let kb = KnowledgeBase::builtin()?;
    let issues = kb.date_issues(today, max_age_days);
    for (entry, issue) in &issues {
        match issue {
            DateIssue::Stale { days } => eprintln!(
                "{}: last verified {} ({days} days ago); re-check its sources",
                entry.id, entry.last_verified
            ),
            DateIssue::InFuture => eprintln!(
                "{}: last_verified {} is in the future",
                entry.id, entry.last_verified
            ),
        }
    }
    println!(
        "{} knowledge-base entries, {} need re-verification",
        kb.entries().len(),
        issues.len()
    );
    Ok(issues.len())
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
        ["devkb", "--max-age-days", days] => devkb(days),
        _ => {
            eprintln!("usage: cargo xtask schema [--check] | devkb --max-age-days <N>");
            return ExitCode::from(2);
        }
    };
    match result {
        Ok(0) => ExitCode::SUCCESS,
        Ok(_) => ExitCode::FAILURE,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}
