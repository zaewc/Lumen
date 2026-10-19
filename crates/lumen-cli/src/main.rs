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
//! lumen finalize --db <FILE> --expired
//! lumen jev key set|clear|status
//! ```
//!
//! `assess` decides the latest completed scan in the database and reports each
//! candidate's verdict and reasons; it moves nothing (roadmap 10.13b). `plan`
//! stores the QUARANTINE candidates as a plan and prints its hash; it moves
//! nothing either (roadmap 10.13c, ADR-0034). `quarantine` executes exactly
//! that plan, re-deciding every item first, and only with its full hash; it
//! moves items into a restorable store (roadmap 10.14b). `list` shows what the
//! store holds, `restore` puts one item back, and `finalize` removes one item
//! for good (roadmap 10.15b), or, with `--expired`, every retained item past
//! its 30-day retention (roadmap 10.20). `jev key` stores, removes or checks the
//! `OpenRouter` API key in the system's secret store, reading it from standard
//! input (roadmap 10.17).
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
#[cfg(any(unix, windows))]
mod held;
#[cfg(any(unix, windows))]
mod jev;
#[cfg(any(unix, windows))]
mod plan;
#[cfg(any(unix, windows))]
mod quarantine;
#[cfg(any(unix, windows))]
mod report;

const USAGE: &str = "usage: lumen scan <PATH>... [--db <FILE>] [--json]\n       lumen assess --db <FILE> [--json]\n       lumen plan --db <FILE>\n       lumen quarantine --db <FILE> --plan <ID> --confirm <HASH>\n       lumen list --db <FILE>\n       lumen restore --db <FILE> --op <ID>\n       lumen finalize --db <FILE> --op <ID>\n       lumen finalize --db <FILE> --expired\n       lumen jev key set|clear|status   (set reads the key from standard input)";

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
    /// Consult Jev for this run (explicit consent, roadmap 10.17b).
    jev: bool,
}

#[derive(Debug, PartialEq, Eq)]
enum Command {
    Scan(ScanArgs),
    Assess(AssessArgs),
    Plan(PlanArgs),
    Quarantine(QuarantineArgs),
    List(PathBuf),
    Restore(OpArgs),
    Finalize(OpArgs),
    FinalizeExpired(PathBuf),
    JevKey(KeyAction),
    Help,
}

/// What to do with the Jev API key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum KeyAction {
    Set,
    Clear,
    Status,
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
    let (db, jev) = match args {
        [flag, path] if flag == "--db" => (path, false),
        [flag, path, j] | [j, flag, path] if flag == "--db" && j == "--jev" => (path, true),
        [flag] if flag == "-h" || flag == "--help" => return Ok(Command::Help),
        _ => return Err("plan needs --db <FILE> [--jev]".into()),
    };
    Ok(Command::Plan(PlanArgs {
        db: PathBuf::from(db),
        jev,
    }))
}

/// Parsed `plan` arguments.
#[derive(Debug, PartialEq, Eq)]
struct PlanArgs {
    db: PathBuf,
    jev: bool,
}

fn parse_assess(args: &[String]) -> Result<Command, String> {
    let mut iter = args.iter();
    let mut db = None;
    let mut json = false;
    let mut jev = false;
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--json" => json = true,
            "--jev" => jev = true,
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
    Ok(Command::Assess(AssessArgs { db, json, jev }))
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
        Some("jev") => {
            return match iter.as_slice() {
                [key, action] if key == "key" => match action.as_str() {
                    "set" => Ok(Command::JevKey(KeyAction::Set)),
                    "clear" => Ok(Command::JevKey(KeyAction::Clear)),
                    "status" => Ok(Command::JevKey(KeyAction::Status)),
                    _ => Err("jev key needs set, clear or status".into()),
                },
                _ => Err("usage: lumen jev key set|clear|status (the key is read from standard input, never an argument)".into()),
            };
        }
        Some("finalize") => {
            return match iter.as_slice() {
                [a, db, b] | [b, a, db] if a == "--db" && b == "--expired" => {
                    Ok(Command::FinalizeExpired(PathBuf::from(db)))
                }
                rest => parse_op(rest).map(Command::Finalize),
            };
        }
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

/// The database a command works on, if any.
fn database(command: &Command) -> Option<&std::path::Path> {
    match command {
        Command::Scan(args) => args.db.as_deref(),
        Command::Assess(args) => Some(&args.db),
        Command::Plan(args) => Some(&args.db),
        Command::Quarantine(args) => Some(&args.db),
        Command::List(db) | Command::FinalizeExpired(db) => Some(db),
        Command::Restore(args) | Command::Finalize(args) => Some(&args.db),
        Command::JevKey(_) | Command::Help => None,
    }
}

/// Local, redacted telemetry (ADR-0022): a rolling JSON log in `logs/` next
/// to the database; nothing is written without one, and nothing leaves the
/// machine. Telemetry never stops a command: a failure only warns.
fn telemetry(command: &Command) -> Option<lumen_telemetry::Telemetry> {
    let dir = database(command)?.with_file_name("logs");
    let config = lumen_telemetry::Config {
        log_dir: Some(&dir),
        ..lumen_telemetry::Config::default()
    };
    lumen_telemetry::start(&config)
        .inspect_err(|e| eprintln!("warning: telemetry is off: {e}"))
        .ok()
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let parsed = parse(&args);
    let _telemetry = parsed.as_ref().ok().and_then(telemetry);
    match parsed {
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
        Ok(Command::JevKey(action)) => report(run_jev_key(action)),
        Ok(Command::Restore(args)) => {
            report(run_held(HeldAction::Restore, &args.db, Some(&args.op)))
        }
        Ok(Command::FinalizeExpired(db)) => {
            report(run_held(HeldAction::FinalizeExpired, &db, None))
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
        Ok(Command::Plan(args)) => match run_plan(&args) {
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
    let summary = lumen_app::scan(
        &args.roots,
        args.db.as_deref(),
        &lumen_app::SystemClock,
        &lumen_app::UuidV7Ids,
        &lumen_application::CancelToken::new(),
    )?;
    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&report::to_json(&summary))?
        );
    } else {
        print!("{}", report::to_text(&summary));
    }
    Ok(())
}

#[cfg(not(any(unix, windows)))]
fn run(_args: &ScanArgs) -> Result<(), Box<dyn Error>> {
    Err("scanning is not supported on this platform".into())
}

/// Runs `with` with Jev when `jev` was asked for, and records its traces.
#[cfg(any(unix, windows))]
fn with_judge<T>(
    jev_wanted: bool,
    db: &std::path::Path,
    with: impl FnOnce(&dyn lumen_application::ports::JudgeModel) -> Result<T, Box<dyn Error>>,
    traces: impl Fn(&T) -> &[lumen_application::assess::Assessed],
) -> Result<T, Box<dyn Error>> {
    if !jev_wanted {
        return with(&lumen_application::ports::NoopJudge);
    }
    let judge = jev::judge()?;
    let result = with(&judge)?;
    jev::record_traces(db, traces(&result))?;
    Ok(result)
}

#[cfg(any(unix, windows))]
fn run_assess(args: &AssessArgs) -> Result<(), Box<dyn Error>> {
    let assessed = with_judge(
        args.jev,
        &args.db,
        |judge| Ok(lumen_app::assess(&args.db, &lumen_app::SystemClock, judge)?),
        Vec::as_slice,
    )?;
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
fn run_plan(args: &PlanArgs) -> Result<(), Box<dyn Error>> {
    let planned = with_judge(
        args.jev,
        &args.db,
        |judge| {
            Ok(lumen_app::plan(
                &args.db,
                &lumen_app::SystemClock,
                &lumen_app::UuidV7Ids,
                judge,
            )?)
        },
        |planned| planned.items.as_slice(),
    )?;
    print!("{}", plan::to_text(&planned, &args.db));
    Ok(())
}

#[cfg(any(unix, windows))]
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
            &lumen_app::SystemClock,
            &lumen_app::UuidV7Ids,
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

#[cfg(any(unix, windows))]
fn run_jev_key(action: KeyAction) -> Result<(), Box<dyn Error>> {
    let text = match action {
        KeyAction::Set => jev::set()?,
        KeyAction::Clear => jev::clear()?,
        KeyAction::Status => jev::status()?,
    };
    print!("{text}");
    Ok(())
}

#[cfg(not(any(unix, windows)))]
fn run_jev_key(_action: KeyAction) -> Result<(), Box<dyn Error>> {
    Err("Jev is not available on this platform".into())
}

/// What to do with held items.
#[derive(Debug, Clone, Copy)]
enum HeldAction {
    List,
    Restore,
    Finalize,
    FinalizeExpired,
}

#[cfg(any(unix, windows))]
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
    let clock = lumen_app::SystemClock;
    let text = match action {
        HeldAction::List => held::list(db, &clock)?,
        HeldAction::Restore => held::restore(db, operation()?, &clock)?,
        HeldAction::Finalize => held::finalize(db, operation()?, &clock)?,
        HeldAction::FinalizeExpired => held::finalize_expired(db, &clock)?,
    };
    print!("{text}");
    Ok(())
}

#[cfg(not(any(unix, windows)))]
fn run_held(
    _action: HeldAction,
    _db: &std::path::Path,
    _op: Option<&str>,
) -> Result<(), Box<dyn Error>> {
    Err("quarantine from the command line is not available on this platform yet".into())
}

#[cfg(not(any(unix, windows)))]
fn run_quarantine(_args: &QuarantineArgs) -> Result<(), Box<dyn Error>> {
    Err("quarantine from the command line is not available on this platform yet".into())
}

#[cfg(not(any(unix, windows)))]
fn run_plan(_args: &PlanArgs) -> Result<(), Box<dyn Error>> {
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
                jev: false,
            }))
        );
        assert!(parse(&args(&["assess"])).is_err(), "needs --db");
        assert_eq!(
            parse(&args(&["plan", "--db", "x.db"])),
            Ok(Command::Plan(PlanArgs {
                db: "x.db".into(),
                jev: false,
            }))
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
        assert_eq!(
            parse(&args(&["finalize", "--expired", "--db", "x.db"])),
            Ok(Command::FinalizeExpired("x.db".into())),
            "only what retention allows"
        );
        assert!(
            parse(&args(&[
                "finalize",
                "--db",
                "x.db",
                "--expired",
                "--op",
                "o"
            ]))
            .is_err()
        );
    }

    #[test]
    fn jev_is_asked_for_per_run() {
        assert_eq!(
            parse(&args(&["plan", "--db", "x.db", "--jev"])),
            Ok(Command::Plan(PlanArgs {
                db: "x.db".into(),
                jev: true,
            }))
        );
        assert!(matches!(
            parse(&args(&["assess", "--jev", "--db", "x.db"])),
            Ok(Command::Assess(AssessArgs { jev: true, .. }))
        ));
    }

    #[test]
    fn the_jev_key_is_never_an_argument() {
        assert_eq!(
            parse(&args(&["jev", "key", "set"])),
            Ok(Command::JevKey(KeyAction::Set))
        );
        assert_eq!(
            parse(&args(&["jev", "key", "status"])),
            Ok(Command::JevKey(KeyAction::Status))
        );
        assert!(parse(&args(&["jev", "key", "set", "sk-or-v1-secret"])).is_err());
        assert!(parse(&args(&["jev", "key"])).is_err());
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
