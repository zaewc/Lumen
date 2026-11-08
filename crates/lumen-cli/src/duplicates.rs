//! `lumen duplicates <PATH>... [--json]`: exact duplicates in chosen folders,
//! for review. Nothing is chosen, moved or removed.

use std::error::Error;
use std::path::PathBuf;

use lumen_app::duplicates::{Duplicates, HashStage, find};
use lumen_domain::FilesystemEntry;

use crate::report::human;

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Args {
    roots: Vec<PathBuf>,
    json: bool,
}

pub(crate) fn parse(args: &[String]) -> Result<Args, String> {
    let mut roots = Vec::new();
    let mut json = false;
    for arg in args {
        match arg.as_str() {
            "--json" => json = true,
            flag if flag.starts_with('-') => return Err(format!("unknown option: {flag}")),
            path => roots.push(PathBuf::from(path)),
        }
    }
    // Contents are read, so the scope is always chosen, never the whole disk.
    if roots.is_empty() {
        return Err("duplicates needs at least one folder to compare".into());
    }
    Ok(Args { roots, json })
}

fn paths(files: &[FilesystemEntry]) -> Vec<String> {
    files.iter().map(|file| file.path.display()).collect()
}

/// Paths and sizes only: content digests stay inside the process (ADR-0019).
fn output(found: &Duplicates) -> serde_json::Value {
    let report = &found.report;
    serde_json::json!({
        "groups": report.groups.iter().map(|group| serde_json::json!({
            "bytes_each": group.files[0].size.logical.get(),
            "paths": paths(&group.files),
        })).collect::<Vec<_>>(),
        "hard_links": report.hard_links.iter().map(|links| paths(links)).collect::<Vec<_>>(),
        "skipped": report.skipped.len(),
        "protected": found.protected,
        "incomplete": found.incomplete,
        "cancelled": report.cancelled,
    })
}

pub(crate) fn run(args: &Args) -> Result<(), Box<dyn Error>> {
    let mut last = None;
    let found = find(
        &args.roots,
        &lumen_application::CancelToken::new(),
        &mut |stage, done| {
            if last != Some(stage) {
                last = Some(stage);
                eprintln!(
                    "comparing {} (files read so far: {done})",
                    match stage {
                        HashStage::Head => "the start of each same-size file",
                        HashStage::Tail => "the end of each candidate",
                        HashStage::Full => "whole candidates",
                    }
                );
            }
        },
    )?;
    if args.json {
        println!("{}", serde_json::to_string(&output(&found))?);
        return Ok(());
    }
    for group in &found.report.groups {
        println!(
            "{} identical copies, {} each (to review):",
            group.files.len(),
            human(group.files[0].size.logical)
        );
        for path in paths(&group.files) {
            println!("  {path}");
        }
    }
    if found.report.groups.is_empty() {
        println!("No identical copies found in the files that could be compared.");
    }
    let notes = [
        (
            found.report.hard_links.len(),
            "file(s) with several names (hard links) are one file, not copies",
        ),
        (
            found.report.skipped.len(),
            "file(s) not compared: changed, unreadable, cloud-only or protected by the system",
        ),
        (
            found.protected,
            "file(s) left unread in protected locations",
        ),
    ];
    for (count, note) in notes {
        if count > 0 {
            println!("{count} {note}");
        }
    }
    if found.incomplete {
        println!("Some folders could not be read completely; files there were not compared.");
    }
    println!(
        "Copies can be backups you want. Lumen does not choose or remove any; space freed by removing one may be less than its size."
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(args: &[&str]) -> Vec<String> {
        args.iter().map(ToString::to_string).collect()
    }

    #[test]
    fn a_folder_is_required_and_no_action_flags_are_accepted() {
        assert_eq!(
            parse(&strings(&["Photos", "--json"])),
            Ok(Args {
                roots: vec![PathBuf::from("Photos")],
                json: true
            })
        );
        for args in [
            vec![],
            vec!["--json"],
            vec!["Photos", "--delete"],
            vec!["Photos", "--keep-newest"],
        ] {
            assert!(parse(&strings(&args)).is_err(), "{args:?}");
        }
    }
}
