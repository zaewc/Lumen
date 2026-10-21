//! `lumen-jev-eval`: Jev's evaluation gate from the command line (roadmap
//! 12.1, docs/ai/harness.md).
//!
//! ```text
//! lumen-jev-eval run    --dataset <VERSION> --cassette <FILE> [--ai <DIR>]
//!                       [--gate full|observed] [--json]
//! lumen-jev-eval record --dataset <VERSION> --out <FILE> [--ai <DIR>]
//! ```
//!
//! `run` loads the dataset version from `ai/` (verified against its manifest),
//! replays the cassette's recorded answers through the production validator
//! and policy, and reports the gates. It exits with 0 when the chosen gate
//! passes, 1 when it fails, and 2 for usage or input errors, so CI can gate on
//! it.
//!
//! - `full` (the default, docs/ai/evaluation.md): the critical false-positive
//!   rate's exact 95% upper bound is at most its target, and no injection
//!   canary is judged more removable than its clean twin. Proving a 0.5% bound
//!   takes about 800 critical cases, so small datasets cannot pass it.
//! - `observed`: no critical false positive was observed in either set, and the
//!   canaries pass. A regression check for datasets too small for `full`; it
//!   is never a release gate.
//!
//! `record` asks Jeb (ADR-0036) about every golden and adversarial case of the
//! dataset, one request at a time, and writes a new cassette that `run`
//! replays. The key comes only from the operating system's secret store, where
//! `lumen jev key set` puts it; it is never an argument, printed or written.
//! The output must not exist yet: nothing is ever overwritten. It exits with 0
//! when every case was recorded, 1 when some were not (the rest are still
//! written), and 2 for usage or input errors.

// A command-line tool reports to the terminal by design.
#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::error::Error;
use std::io::Write as _;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use lumen_app::ai::Service;
use lumen_application::ports::JudgeModel as _;
use lumen_domain::{JevRequest, JudgeDescriptor};
use lumen_jev::openrouter::{ApiKey, AskError, JevJudge};
use lumen_jev_eval::{
    CRITICAL_FPR_TARGET, Cassette, DatasetReport, LiveJudge, Pacing, RecordOutcome, ReplayJudge,
    Report, load, record, run_dataset,
};

const USAGE: &str = "usage: lumen-jev-eval run --dataset <VERSION> --cassette <FILE> [--ai <DIR>] [--gate full|observed] [--json]
       lumen-jev-eval record --dataset <VERSION> --out <FILE> [--ai <DIR>]";

/// Which gate decides the exit status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Gate {
    /// The statistical release gate.
    Full,
    /// No observed critical false positive, and clean canaries.
    Observed,
}

/// Parsed `run` arguments.
#[derive(Debug, PartialEq, Eq)]
struct RunArgs {
    dataset: String,
    cassette: PathBuf,
    ai: PathBuf,
    gate: Gate,
    json: bool,
}

/// Parsed `record` arguments.
#[derive(Debug, PartialEq, Eq)]
struct RecordArgs {
    dataset: String,
    out: PathBuf,
    ai: PathBuf,
}

/// A parsed command line.
#[derive(Debug, PartialEq, Eq)]
enum Command {
    Run(RunArgs),
    Record(RecordArgs),
}

fn parse(args: &[String]) -> Result<Command, String> {
    let Some((command, rest)) = args.split_first() else {
        return Err("a command is needed".into());
    };
    let record = match command.as_str() {
        "run" => false,
        "record" => true,
        _ => return Err(format!("unknown command `{command}`")),
    };
    let (mut dataset, mut cassette, mut out, mut ai, mut json) =
        (None, None, None, PathBuf::from("ai"), false);
    let mut gate = Gate::Full;
    let mut iter = rest.iter();
    while let Some(arg) = iter.next() {
        let mut value = || {
            iter.next()
                .cloned()
                .ok_or_else(|| format!("{arg} needs a value"))
        };
        match (arg.as_str(), record) {
            ("--dataset", _) => dataset = Some(value()?),
            ("--ai", _) => ai = PathBuf::from(value()?),
            ("--cassette", false) => cassette = Some(PathBuf::from(value()?)),
            ("--gate", false) => {
                gate = match value()?.as_str() {
                    "full" => Gate::Full,
                    "observed" => Gate::Observed,
                    other => return Err(format!("unknown gate `{other}`")),
                };
            }
            ("--json", false) => json = true,
            ("--out", true) => out = Some(PathBuf::from(value()?)),
            (other, _) => return Err(format!("unexpected argument `{other}`")),
        }
    }
    let dataset = dataset.ok_or("--dataset is needed")?;
    Ok(if record {
        Command::Record(RecordArgs {
            dataset,
            out: out.ok_or("--out is needed")?,
            ai,
        })
    } else {
        Command::Run(RunArgs {
            dataset,
            cassette: cassette.ok_or("--cassette is needed")?,
            ai,
            gate,
            json,
        })
    })
}

/// Whether `report` passes `gate`.
fn passes(report: &DatasetReport, gate: Gate) -> bool {
    match gate {
        Gate::Full => report.passed,
        Gate::Observed => {
            report.golden.critical_false_positives == 0
                && report.adversarial.critical_false_positives == 0
                && report.canary_flips.is_empty()
        }
    }
}

fn percent(value: Option<f64>) -> String {
    value.map_or_else(|| "n/a".into(), |v| format!("{:.1}%", v * 100.0))
}

fn summary(name: &str, report: &Report) -> String {
    format!(
        "{name}: {} cases; critical false positives {}/{} (95% upper bound {:.2}%, target {:.2}%); precision {}, recall {}, abstention {}; {}",
        report.cases,
        report.critical_false_positives,
        report.critical_cases,
        report.critical_fpr_upper * 100.0,
        CRITICAL_FPR_TARGET * 100.0,
        percent(report.precision),
        percent(report.recall),
        percent(report.abstention_rate),
        if report.passed { "pass" } else { "FAIL" },
    )
}

fn text(report: &DatasetReport, gate: Gate) -> String {
    let mut lines = vec![
        format!("dataset {}", report.version),
        summary("golden", &report.golden),
        summary("adversarial", &report.adversarial),
    ];
    lines.push(if report.canary_flips.is_empty() {
        "canaries: none judged more removable than their clean twin; pass".into()
    } else {
        format!(
            "canaries: FAIL, judged more removable than their twin: {}",
            report.canary_flips.join(", ")
        )
    });
    let verdict = if passes(report, gate) { "PASS" } else { "FAIL" };
    lines.push(match gate {
        Gate::Full => format!("release gate (full): {verdict}"),
        Gate::Observed => format!(
            "observed gate (no critical false positive seen, clean canaries; not a release gate): {verdict}"
        ),
    });
    lines.join("\n") + "\n"
}

fn run(args: &RunArgs) -> Result<DatasetReport, Box<dyn Error>> {
    let dataset = load(&args.ai, &args.dataset)?;
    let cassette = Cassette::parse(&std::fs::read_to_string(&args.cassette)?)?;
    Ok(run_dataset(&ReplayJudge::new(cassette), &dataset))
}

/// Jeb through the production transport, paced in real time.
struct Jeb {
    judge: JevJudge,
    descriptor: JudgeDescriptor,
}

impl LiveJudge for Jeb {
    fn descriptor(&self) -> JudgeDescriptor {
        self.descriptor.clone()
    }

    fn ask(&self, request: &JevRequest) -> Result<String, AskError> {
        self.judge.ask(request)
    }

    fn wait(&self, pause: Duration) {
        std::thread::sleep(pause);
    }
}

/// The Jeb key from the operating system's secret store, looked up as the CLI
/// does (`lumen_app::ai::stored_key`): the slot `lumen jev key set` writes.
/// Errors never contain the key.
fn jeb_key() -> Result<ApiKey, String> {
    let store = lumen_app::ai::os_store()
        .ok_or("no secret store on this platform; recording needs the macOS keychain or the Windows Credential Manager")?;
    match lumen_app::ai::stored_key(store.as_ref()) {
        Ok(Some((Service::Jeb, key))) => Ok(key),
        Ok(_) => Err("no Jeb API key is stored: run `lumen jev key set` (it reads the key from standard input) first".into()),
        Err(_) => Err("the Jeb API key could not be read from the secret store (allow access if the keychain asks), or it is not valid: store it again with `lumen jev key set`".into()),
    }
}

fn describe(outcome: &RecordOutcome) -> String {
    match outcome {
        RecordOutcome::Recorded => "recorded".into(),
        RecordOutcome::Refused(e) => format!("refused by the validator, recorded ({e})"),
        RecordOutcome::Failed(e) => format!("failed, not recorded ({e})"),
        RecordOutcome::NotSent => "not sent (the key was rejected)".into(),
    }
}

/// Records a Jeb cassette; `Err` is bad input (exit 2), `Ok(false)` an
/// incomplete recording (exit 1).
fn record_jeb(args: &RecordArgs) -> Result<bool, Box<dyn Error>> {
    let dataset = load(&args.ai, &args.dataset)?;
    if args.out.try_exists()? {
        return Err(format!(
            "{} already exists; choose a new file (nothing is overwritten)",
            args.out.display()
        )
        .into());
    }
    let judge = Service::Jeb.judge(jeb_key()?)?;
    let descriptor = judge
        .descriptor()
        .ok_or("the Jeb judge has no descriptor")?;
    // Reserve the output before spending quota; never replace a file.
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&args.out)
        .map_err(|e| format!("cannot create {}: {e}", args.out.display()))?;
    let cases = dataset.golden.len() + dataset.adversarial.len();
    eprintln!("{}", Service::Jeb.disclosure());
    eprintln!(
        "Recording dataset {}: {cases} requests to Jeb ({}), one at a time. The cases are synthetic metadata from the dataset, never file contents.",
        dataset.version,
        lumen_jev::decisions::JEB_ENDPOINT,
    );
    let recording = record(
        &Jeb { judge, descriptor },
        &dataset,
        Pacing::JEB,
        &mut |case_id, outcome| eprintln!("{case_id}: {}", describe(outcome)),
    );
    let mut text = serde_json::to_string_pretty(&recording.cassette)?;
    text.push('\n');
    file.write_all(text.as_bytes())?;
    file.sync_all()?;
    let count =
        |f: fn(&RecordOutcome) -> bool| recording.outcomes.iter().filter(|(_, o)| f(o)).count();
    eprintln!(
        "{} of {cases} cases recorded ({} refused by the validator), {} not recorded; wrote {}",
        count(RecordOutcome::is_recorded),
        count(|o| matches!(o, RecordOutcome::Refused(_))),
        count(|o| !o.is_recorded()),
        args.out.display(),
    );
    Ok(recording.complete())
}

fn report_run(parsed: &RunArgs) -> ExitCode {
    match run(parsed) {
        Ok(report) => {
            if parsed.json {
                match serde_json::to_string_pretty(&report) {
                    Ok(json) => println!("{json}"),
                    Err(e) => {
                        eprintln!("error: {e}");
                        return ExitCode::from(2);
                    }
                }
            } else {
                print!("{}", text(&report, parsed.gate));
            }
            if passes(&report, parsed.gate) {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            }
        }
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::from(2)
        }
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match parse(&args) {
        Ok(Command::Run(parsed)) => report_run(&parsed),
        Ok(Command::Record(parsed)) => match record_jeb(&parsed) {
            Ok(true) => ExitCode::SUCCESS,
            Ok(false) => ExitCode::FAILURE,
            Err(e) => {
                eprintln!("error: {e}");
                ExitCode::from(2)
            }
        },
        Err(message) => {
            eprintln!("error: {message}\n{USAGE}");
            ExitCode::from(2)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn parses_run_and_refuses_the_rest() {
        assert_eq!(
            parse(&args(&[
                "run",
                "--dataset",
                "2026.10.0",
                "--cassette",
                "c.json",
                "--json"
            ])),
            Ok(Command::Run(RunArgs {
                dataset: "2026.10.0".into(),
                cassette: "c.json".into(),
                ai: "ai".into(),
                gate: Gate::Full,
                json: true,
            }))
        );
        assert!(matches!(
            parse(&args(&[
                "run",
                "--dataset",
                "d",
                "--cassette",
                "c",
                "--gate",
                "observed"
            ])),
            Ok(Command::Run(RunArgs {
                gate: Gate::Observed,
                ..
            }))
        ));
        assert!(
            parse(&args(&[
                "run",
                "--dataset",
                "d",
                "--cassette",
                "c",
                "--gate",
                "x"
            ]))
            .is_err()
        );
        assert!(parse(&args(&["run", "--dataset", "2026.10.0"])).is_err());
        assert!(parse(&args(&["run", "--dataset", "d", "--out", "o"])).is_err());
        assert!(parse(&args(&["run", "--cassette"])).is_err());
        assert!(parse(&args(&["bogus"])).is_err());
        assert!(parse(&[]).is_err());
    }

    #[test]
    fn parses_record_without_a_key_or_force() {
        assert_eq!(
            parse(&args(&[
                "record",
                "--dataset",
                "2026.10.0",
                "--out",
                "jeb.json",
                "--ai",
                "data"
            ])),
            Ok(Command::Record(RecordArgs {
                dataset: "2026.10.0".into(),
                out: "jeb.json".into(),
                ai: "data".into(),
            }))
        );
        assert!(matches!(
            parse(&args(&["record", "--dataset", "d", "--out", "o"])),
            Ok(Command::Record(RecordArgs { ai, .. })) if ai == std::path::Path::new("ai")
        ));
        for bad in [
            &["record"][..],
            &["record", "--dataset", "d"],
            &["record", "--out", "o"],
            &["record", "--dataset", "d", "--out"],
            &["record", "--dataset", "d", "--out", "o", "--cassette", "c"],
            &["record", "--dataset", "d", "--out", "o", "--gate", "full"],
            &["record", "--dataset", "d", "--out", "o", "--force"],
            &[
                "record",
                "--dataset",
                "d",
                "--out",
                "o",
                "--key",
                "sk-secret",
            ],
        ] {
            assert!(parse(&args(bad)).is_err(), "{bad:?}");
        }
    }
}
