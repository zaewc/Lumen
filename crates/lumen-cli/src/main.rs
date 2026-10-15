//! `lumen`: the developer command line.
//!
//! ```text
//! lumen scan <PATH>... [--db <FILE>] [--json]
//! lumen assess --db <FILE> [--json]
//! lumen plan --db <FILE>
//! lumen quarantine --db <FILE> --plan <ID> --confirm <HASH>
//! lumen list --db <FILE>
//! lumen restore --db <FILE> --op <ID>
//! lumen finalize --db <FILE> --op <ID>
//! ```
//!
//! `assess` decides the latest completed scan in the database and reports each
//! candidate's verdict and reasons; it moves nothing (roadmap 10.13b). `plan`
//! stores the QUARANTINE candidates as a plan and prints its hash; it moves
//! nothing either (roadmap 10.13c, ADR-0034). `quarantine` executes exactly
//! that plan, re-deciding every item first, and only with its full hash; it
//! moves items into a restorable store (roadmap 10.14b). `list` shows what the
//! store holds, `restore` puts one item back, and `finalize` removes one item
//! for good (roadmap 10.15b).
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

#[cfg(any(unix, windows))]
mod assess;
#[cfg(unix)]
mod held;
#[cfg(any(unix, windows))]
mod plan;
#[cfg(unix)]
mod quarantine;
#[cfg(any(unix, windows))]
mod report;
#[cfg(any(unix, windows))]
mod system;

const USAGE: &str = "usage: lumen scan <PATH>... [--db <FILE>] [--json]\n       lumen assess --db <FILE> [--json]\n       lumen plan --db <FILE>\n       lumen quarantine --db <FILE> --plan <ID> --confirm <HASH>\n       lumen list --db <FILE>\n       lumen restore --db <FILE> --op <ID>\n       lumen finalize --db <FILE> --op <ID>";

/// Parsed `scan` arguments.
#[derive(Debug, PartialEq, Eq)]
struct ScanArgs {
    roots: Vec<PathBuf>,
    db: Option<PathBuf>,
    json: bool,
}

/// Parsed `assess` arguments.
#[derive(Debug, PartialEq, Eq)]
struct AssessArgs {
    db: PathBuf,
    json: bool,
}

#[derive(Debug, PartialEq, Eq)]
enum Command {
    Scan(ScanArgs),
    Assess(AssessArgs),
    Plan(PathBuf),
    Quarantine(QuarantineArgs),
    List(PathBuf),
    Restore(OpArgs),
    Finalize(OpArgs),
    Help,
}

/// Parsed arguments naming one quarantine operation.
#[derive(Debug, PartialEq, Eq)]
struct OpArgs {
    db: PathBuf,
    op: String,
}

fn parse_op(args: &[String]) -> Result<OpArgs, String> {
    match args {
        [a, db, b, op] if a == "--db" && b == "--op" => Ok(OpArgs {
            db: PathBuf::from(db),
            op: op.clone(),
        }),
        [a, op, b, db] if a == "--op" && b == "--db" => Ok(OpArgs {
            db: PathBuf::from(db),
            op: op.clone(),
        }),
        _ => Err("needs exactly --db <FILE> --op <ID>".into()),
    }
}

/// Parsed `quarantine` arguments.
#[derive(Debug, PartialEq, Eq)]
struct QuarantineArgs {
    db: PathBuf,
    plan: String,
    confirm: String,
}

fn parse_quarantine(args: &[String]) -> Result<Command, String> {
    let mut iter = args.iter();
    let (mut db, mut plan, mut confirm) = (None, None, None);
    while let Some(arg) = iter.next() {
        let slot = match arg.as_str() {
            "--db" => &mut db,
            "--plan" => &mut plan,
            "--confirm" => &mut confirm,
            "-h" | "--help" => return Ok(Command::Help),
            other => return Err(format!("unknown argument: {other}")),
        };
        let value = iter.next().ok_or_else(|| format!("{arg} needs a value"))?;
        if slot.replace(value.clone()).is_some() {
            return Err(format!("{arg} given more than once"));
        }
    }
    match (db, plan, confirm) {
        (Some(db), Some(plan), Some(confirm)) => Ok(Command::Quarantine(QuarantineArgs {
            db: PathBuf::from(db),
            plan,
            confirm,
        })),
        _ => Err("quarantine needs --db <FILE> --plan <ID> --confirm <HASH>".into()),
    }
}

fn parse_plan(args: &[String]) -> Result<Command, String> {
    match args {
        [flag, path] if flag == "--db" => Ok(Command::Plan(PathBuf::from(path))),
        [flag] if flag == "-h" || flag == "--help" => Ok(Command::Help),
        _ => Err("plan needs exactly --db <FILE>".into()),
    }
}

fn parse_assess(args: &[String]) -> Result<Command, String> {
    let mut iter = args.iter();
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
            other => return Err(format!("unknown argument: {other}")),
        }
    }
    let db = db.ok_or("assess needs --db <FILE>")?;
    Ok(Command::Assess(AssessArgs { db, json }))
}

fn parse(args: &[String]) -> Result<Command, String> {
    let mut iter = args.iter();
    match iter.next().map(String::as_str) {
        Some("scan") => {}
        Some("assess") => return parse_assess(iter.as_slice()),
        Some("plan") => return parse_plan(iter.as_slice()),
        Some("quarantine") => return parse_quarantine(iter.as_slice()),
        Some("list") => {
            return match iter.as_slice() {
                [flag, db] if flag == "--db" => Ok(Command::List(PathBuf::from(db))),
                _ => Err("list needs exactly --db <FILE>".into()),
            };
        }
        Some("restore") => return parse_op(iter.as_slice()).map(Command::Restore),
        Some("finalize") => return parse_op(iter.as_slice()).map(Command::Finalize),
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
        Ok(Command::List(db)) => report(run_held(HeldAction::List, &db, None)),
        Ok(Command::Restore(args)) => {
            report(run_held(HeldAction::Restore, &args.db, Some(&args.op)))
        }
        Ok(Command::Finalize(args)) => {
            report(run_held(HeldAction::Finalize, &args.db, Some(&args.op)))
        }
        Ok(Command::Quarantine(args)) => match run_quarantine(&args) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("error: {e}");
                ExitCode::FAILURE
            }
        },
        Ok(Command::Plan(db)) => match run_plan(&db) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("error: {e}");
                ExitCode::FAILURE
            }
        },
        Ok(Command::Assess(assess)) => match run_assess(&assess) {
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

#[cfg(any(unix, windows))]
fn run(args: &ScanArgs) -> Result<(), Box<dyn Error>> {
    let report = report::scan(
        &args.roots,
        args.db.as_deref(),
        &system::SystemClock,
        &system::UuidV7Ids,
    )?;
    if args.json {
        println!("{}", serde_json::to_string_pretty(&report.to_json())?);
    } else {
        print!("{}", report.to_text());
    }
    Ok(())
}

#[cfg(not(any(unix, windows)))]
fn run(_args: &ScanArgs) -> Result<(), Box<dyn Error>> {
    Err("scanning is not supported on this platform".into())
}

#[cfg(any(unix, windows))]
fn run_assess(args: &AssessArgs) -> Result<(), Box<dyn Error>> {
    let assessed = assess::assess(&args.db, &system::SystemClock)?;
    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&assess::to_json(&assessed))?
        );
    } else {
        print!("{}", assess::to_text(&assessed));
    }
    Ok(())
}

#[cfg(not(any(unix, windows)))]
fn run_assess(_args: &AssessArgs) -> Result<(), Box<dyn Error>> {
    Err("assessment is not supported on this platform".into())
}

#[cfg(any(unix, windows))]
fn run_plan(db: &std::path::Path) -> Result<(), Box<dyn Error>> {
    let planned = plan::plan(db, &system::SystemClock, &system::UuidV7Ids)?;
    print!("{}", plan::to_text(&planned, db));
    Ok(())
}

#[cfg(unix)]
fn run_quarantine(args: &QuarantineArgs) -> Result<(), Box<dyn Error>> {
    let plan = args
        .plan
        .parse()
        .map_err(|_| format!("not a plan ID: {}", args.plan))?;
    print!(
        "{}",
        quarantine::quarantine(
            &args.db,
            plan,
            &args.confirm,
            &system::SystemClock,
            &system::UuidV7Ids,
        )?
    );
    Ok(())
}

/// Prints an error and exits with failure, or exits with success.
fn report(result: Result<(), Box<dyn Error>>) -> ExitCode {
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

/// What to do with held items.
#[derive(Debug, Clone, Copy)]
enum HeldAction {
    List,
    Restore,
    Finalize,
}

#[cfg(unix)]
fn run_held(
    action: HeldAction,
    db: &std::path::Path,
    op: Option<&str>,
) -> Result<(), Box<dyn Error>> {
    let operation = || -> Result<lumen_domain::OperationId, Box<dyn Error>> {
        let op = op.ok_or("an operation ID is needed")?;
        op.parse()
            .map_err(|_| format!("not an operation ID: {op}").into())
    };
    let clock = system::SystemClock;
    let text = match action {
        HeldAction::List => held::list(db, &clock)?,
        HeldAction::Restore => held::restore(db, operation()?, &clock)?,
        HeldAction::Finalize => held::finalize(db, operation()?, &clock)?,
    };
    print!("{text}");
    Ok(())
}

#[cfg(not(unix))]
fn run_held(
    _action: HeldAction,
    _db: &std::path::Path,
    _op: Option<&str>,
) -> Result<(), Box<dyn Error>> {
    Err("quarantine from the command line is not available on this platform yet".into())
}

#[cfg(not(unix))]
fn run_quarantine(_args: &QuarantineArgs) -> Result<(), Box<dyn Error>> {
    Err("quarantine from the command line is not available on this platform yet".into())
}

#[cfg(not(any(unix, windows)))]
fn run_plan(_db: &std::path::Path) -> Result<(), Box<dyn Error>> {
    Err("planning is not supported on this platform".into())
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
    fn parses_assess_arguments() {
        assert_eq!(
            parse(&args(&["assess", "--db", "x.db", "--json"])),
            Ok(Command::Assess(AssessArgs {
                db: "x.db".into(),
                json: true,
            }))
        );
        assert!(parse(&args(&["assess"])).is_err(), "needs --db");
        assert_eq!(
            parse(&args(&["plan", "--db", "x.db"])),
            Ok(Command::Plan("x.db".into()))
        );
        assert!(parse(&args(&["plan"])).is_err());
        assert!(
            parse(&args(&["plan", "--db", "x.db", "--yes"])).is_err(),
            "no shortcut"
        );
        assert_eq!(
            parse(&args(&[
                "quarantine",
                "--db",
                "x.db",
                "--plan",
                "p",
                "--confirm",
                "h"
            ])),
            Ok(Command::Quarantine(QuarantineArgs {
                db: "x.db".into(),
                plan: "p".into(),
                confirm: "h".into(),
            }))
        );
        for missing in [
            &["quarantine", "--db", "x.db", "--plan", "p"][..],
            &["quarantine", "--db", "x.db", "--confirm", "h"],
            &["quarantine", "--plan", "p", "--confirm", "h"],
        ] {
            assert!(parse(&args(missing)).is_err(), "{missing:?}");
        }
        assert!(
            parse(&args(&[
                "quarantine",
                "--db",
                "x",
                "--plan",
                "p",
                "--confirm",
                "h",
                "--yes"
            ]))
            .is_err(),
            "no shortcut"
        );
        assert!(parse(&args(&["assess", "--db", "x.db", "/a"])).is_err());
    }

    #[test]
    fn parses_list_restore_and_finalize() {
        assert_eq!(
            parse(&args(&["list", "--db", "x.db"])),
            Ok(Command::List("x.db".into()))
        );
        let op = OpArgs {
            db: "x.db".into(),
            op: "o".into(),
        };
        assert_eq!(
            parse(&args(&["restore", "--db", "x.db", "--op", "o"])),
            Ok(Command::Restore(op))
        );
        assert!(matches!(
            parse(&args(&["finalize", "--op", "o", "--db", "x.db"])),
            Ok(Command::Finalize(_))
        ));
        assert!(
            parse(&args(&["finalize", "--db", "x.db"])).is_err(),
            "one named operation"
        );
        assert!(
            parse(&args(&["finalize", "--db", "x.db", "--all"])).is_err(),
            "no bulk removal"
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
