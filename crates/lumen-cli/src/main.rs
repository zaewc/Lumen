//! `lumen`: the developer command line.
//!
//! ```text
//! lumen scan <PATH>... [--db <FILE>] [--json]
//! ```
//!
//! Scans are **read-only**: Lumen never modifies the scanned files. With `--db`,
//! results and a snapshot are written to that `index.db` file (and nothing else).
//! If the process is interrupted, the scan stays `running` in the database and can
//! be resumed later.

// A command-line tool reports to the terminal by design.
#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::error::Error;
use std::path::PathBuf;
use std::process::ExitCode;

#[cfg(unix)]
mod report;

const USAGE: &str = "usage: lumen scan <PATH>... [--db <FILE>] [--json]";

/// Parsed `scan` arguments.
#[derive(Debug, PartialEq, Eq)]
struct ScanArgs {
    roots: Vec<PathBuf>,
    db: Option<PathBuf>,
    json: bool,
}

#[derive(Debug, PartialEq, Eq)]
enum Command {
    Scan(ScanArgs),
    Help,
}

fn parse(args: &[String]) -> Result<Command, String> {
    let mut iter = args.iter();
    match iter.next().map(String::as_str) {
        Some("scan") => {}
        Some("-h" | "--help" | "help") | None => return Ok(Command::Help),
        Some(other) => return Err(format!("unknown command: {other}")),
    }
    let mut roots = Vec::new();
    let mut db = None;
    let mut json = false;
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--json" => json = true,
            "--db" => {
                let path = iter.next().ok_or("--db needs a file path")?;
                if db.replace(PathBuf::from(path)).is_some() {
                    return Err("--db given more than once".into());
                }
            }
            "-h" | "--help" => return Ok(Command::Help),
            flag if flag.starts_with('-') => return Err(format!("unknown option: {flag}")),
            path => roots.push(PathBuf::from(path)),
        }
    }
    if roots.is_empty() {
        return Err("scan needs at least one path".into());
    }
    Ok(Command::Scan(ScanArgs { roots, db, json }))
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match parse(&args) {
        Ok(Command::Help) => {
            println!("{USAGE}");
            ExitCode::SUCCESS
        }
        Ok(Command::Scan(scan)) => match run(&scan) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("error: {e}");
                ExitCode::FAILURE
            }
        },
        Err(message) => {
            eprintln!("error: {message}\n{USAGE}");
            ExitCode::from(2)
        }
    }
}

#[cfg(unix)]
fn run(args: &ScanArgs) -> Result<(), Box<dyn Error>> {
    let report = report::scan(&args.roots, args.db.as_deref())?;
    if args.json {
        println!("{}", serde_json::to_string_pretty(&report.to_json())?);
    } else {
        print!("{}", report.to_text());
    }
    Ok(())
}

#[cfg(not(unix))]
fn run(_args: &ScanArgs) -> Result<(), Box<dyn Error>> {
    // The portable enumerator needs Unix file identities; the native Windows
    // enumerator arrives with roadmap item 5.14.
    Err("scanning is not supported on this platform yet".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn parses_scan_arguments() {
        assert_eq!(
            parse(&args(&["scan", "/a", "--json", "/b", "--db", "x.db"])),
            Ok(Command::Scan(ScanArgs {
                roots: vec!["/a".into(), "/b".into()],
                db: Some("x.db".into()),
                json: true,
            }))
        );
    }

    #[test]
    fn rejects_bad_usage() {
        assert!(parse(&args(&["scan"])).is_err(), "needs a path");
        assert!(
            parse(&args(&["scan", "/a", "--db"])).is_err(),
            "--db needs a value"
        );
        assert!(parse(&args(&["scan", "/a", "--db", "a", "--db", "b"])).is_err());
        assert!(parse(&args(&["scan", "/a", "--force"])).is_err());
        assert!(
            parse(&args(&["delete", "/a"])).is_err(),
            "there is no destructive command"
        );
        assert_eq!(parse(&args(&[])), Ok(Command::Help));
    }
}
