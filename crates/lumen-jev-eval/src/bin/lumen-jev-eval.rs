//! `lumen-jev-eval`: Jev's evaluation gate from the command line (roadmap
//! 12.1, docs/ai/harness.md).
//!
//! ```text
//! lumen-jev-eval run    --dataset <VERSION> --cassette <FILE> [--split <NAME>]
//!                       [--prompt v1|v2] [--ai <DIR>] [--gate full|observed] [--json]
//! lumen-jev-eval record --dataset <VERSION> --out <FILE> [--split <NAME>]
//!                       [--prompt v1|v2] [--ai <DIR>] [--resume <FILE>]
//! ```
//!
//! `run` loads the dataset version from `ai/` (verified against its manifest),
//! replays the cassette's recorded answers through the production validator
//! (with the prompt version the cassette records) and policy, and reports the
//! gates. `--split` keeps one split's golden cases and their canaries;
//! `--prompt` refuses a cassette recorded with another prompt. It exits with 0 when the chosen gate
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
//! dataset (or of one `--split`), one request at a time, with the questions of
//! `--prompt` (`v1`, the app's prompt, by default), and writes a new cassette
//! that `run` replays. The key comes only from the operating system's secret store, where
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
use lumen_jev::decisions::Prompt;
use lumen_jev::openrouter::{ApiKey, JevJudge};
use lumen_jev_eval::{
    CRITICAL_FPR_TARGET, Cassette, CassetteError, Dataset, DatasetReport, LiveError, LiveJudge,
    Pacing, RecordOutcome, ReplayJudge, Report, load, record, run_dataset,
};

const USAGE: &str = "usage: lumen-jev-eval run --dataset <VERSION> --cassette <FILE> [--split <NAME>] [--prompt v1|v2] [--ai <DIR>] [--gate full|observed] [--json]
       lumen-jev-eval record --dataset <VERSION> --out <FILE> [--split <NAME>] [--prompt v1|v2] [--ai <DIR>] [--resume <FILE>]
splits: dev, calibration, test-locked (canaries go with their twin's split)";

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
    /// Only this split's cases.
    split: Option<String>,
    /// The prompt the cassette must have been recorded with.
    prompt: Option<Prompt>,
    ai: PathBuf,
    gate: Gate,
    json: bool,
}

/// Parsed `record` arguments.
#[derive(Debug, PartialEq, Eq)]
struct RecordArgs {
    dataset: String,
    out: PathBuf,
    /// Only this split's cases.
    split: Option<String>,
    /// The prompt to ask (v1 unless given).
    prompt: Prompt,
    ai: PathBuf,
    /// An earlier, incomplete recording whose answers are kept.
    resume: Option<PathBuf>,
}

/// `v1` or `v2`.
fn prompt(value: &str) -> Result<Prompt, String> {
    match value {
        "v1" => Ok(Prompt::V1),
        "v2" => Ok(Prompt::V2),
        other => Err(format!("unknown prompt `{other}` (v1 or v2)")),
    }
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
    let (mut resume, mut split, mut chosen_prompt) = (None, None, None);
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
            ("--split", _) => split = Some(value()?),
            ("--prompt", _) => chosen_prompt = Some(prompt(&value()?)?),
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
            ("--resume", true) => resume = Some(PathBuf::from(value()?)),
            (other, _) => return Err(format!("unexpected argument `{other}`")),
        }
    }
    let dataset = dataset.ok_or("--dataset is needed")?;
    Ok(if record {
        Command::Record(RecordArgs {
            dataset,
            out: out.ok_or("--out is needed")?,
            split,
            prompt: chosen_prompt.unwrap_or_default(),
            ai,
            resume,
        })
    } else {
        Command::Run(RunArgs {
            dataset,
            cassette: cassette.ok_or("--cassette is needed")?,
            split,
            prompt: chosen_prompt,
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
                // Nothing observed is not a pass: every critical case and every
                // canary must have been answered.
                && report.golden.critical_unanswered == 0
                && report.adversarial.unanswered == 0
        }
    }
}

fn percent(value: Option<f64>) -> String {
    value.map_or_else(|| "n/a".into(), |v| format!("{:.1}%", v * 100.0))
}

fn summary(name: &str, report: &Report) -> String {
    format!(
        "{name}: {} cases, {} unanswered; critical false positives {}/{} answered (95% upper bound {:.2}%, target {:.2}%); precision {}, recall {}, abstention {}; {}",
        report.cases,
        report.unanswered,
        report.critical_false_positives,
        report.critical_cases - report.critical_unanswered,
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
        match &report.split {
            Some(split) => format!("dataset {}, split {split}", report.version),
            None => format!("dataset {}", report.version),
        },
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

/// The dataset version, restricted to `split` if one is given.
fn dataset(
    ai: &std::path::Path,
    version: &str,
    split: Option<&str>,
) -> Result<Dataset, Box<dyn Error>> {
    let dataset = load(ai, version)?;
    Ok(match split {
        Some(name) => dataset.split(name)?,
        None => dataset,
    })
}

fn run(args: &RunArgs) -> Result<DatasetReport, Box<dyn Error>> {
    let dataset = dataset(&args.ai, &args.dataset, args.split.as_deref())?;
    let cassette = Cassette::parse(&std::fs::read_to_string(&args.cassette)?)?;
    if let Some(prompt) = args.prompt {
        cassette.expect_prompt(prompt)?;
    }
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

    fn ask(&self, request: &JevRequest) -> Result<String, LiveError> {
        Ok(self.judge.ask(request)?)
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
        RecordOutcome::NotSent => {
            "not sent (an earlier error showed no further request can succeed)".into()
        }
        RecordOutcome::Kept => "kept from the earlier recording".into(),
    }
}

/// Records a Jeb cassette; `Err` is bad input (exit 2), `Ok(false)` an
/// incomplete recording (exit 1).
fn record_jeb(args: &RecordArgs) -> Result<bool, Box<dyn Error>> {
    let dataset = dataset(&args.ai, &args.dataset, args.split.as_deref())?;
    if args.out.try_exists()? {
        return Err(format!(
            "{} already exists; choose a new file (nothing is overwritten)",
            args.out.display()
        )
        .into());
    }
    let previous = match &args.resume {
        Some(path) => Some(Cassette::parse(&std::fs::read_to_string(path)?)?),
        None => None,
    };
    let judge = Service::Jeb.judge(jeb_key()?)?.with_prompt(args.prompt)?;
    let descriptor = judge
        .descriptor()
        .ok_or("the Jeb judge has no descriptor")?;
    if previous.as_ref().is_some_and(|p| p.judge != descriptor) {
        return Err(CassetteError::OtherJudge.into());
    }
    // Reserve the output before spending quota; never replace a file.
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&args.out)
        .map_err(|e| format!("cannot create {}: {e}", args.out.display()))?;
    let cases = dataset.golden.len() + dataset.adversarial.len();
    let have = previous.as_ref().map_or(0, |p| {
        dataset
            .cases()
            .filter(|c| {
                p.entries
                    .iter()
                    .any(|e| e.case_id == c.case_id && e.request_hash == c.request.hash())
            })
            .count()
    });
    eprintln!("{}", Service::Jeb.disclosure());
    eprintln!(
        "Recording dataset {}{}: {} requests to Jeb ({}) with prompt {}, one at a time ({have} kept from the earlier recording). The cases are synthetic metadata from the dataset, never file contents.",
        dataset.version,
        dataset
            .split
            .as_deref()
            .map_or_else(String::new, |s| format!(", split {s}")),
        cases - have,
        lumen_jev::decisions::JEB_ENDPOINT,
        args.prompt.version(),
    );
    let recording = record(
        &Jeb { judge, descriptor },
        &dataset,
        Pacing::JEB,
        previous.as_ref(),
        &mut |case_id, outcome| {
            if *outcome != RecordOutcome::Kept {
                eprintln!("{case_id}: {}", describe(outcome));
            }
        },
    )?;
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
                split: None,
                prompt: None,
                ai: "ai".into(),
                gate: Gate::Full,
                json: true,
            }))
        );
        assert!(matches!(
            parse(&args(&[
                "run", "--dataset", "d", "--cassette", "c", "--split", "dev", "--prompt", "v2"
            ])),
            Ok(Command::Run(RunArgs { split: Some(s), prompt: Some(Prompt::V2), .. })) if s == "dev"
        ));
        assert!(
            parse(&args(&[
                "run",
                "--dataset",
                "d",
                "--cassette",
                "c",
                "--prompt",
                "v3"
            ]))
            .is_err()
        );
        assert!(
            parse(&args(&[
                "run",
                "--dataset",
                "d",
                "--cassette",
                "c",
                "--split"
            ]))
            .is_err()
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
                split: None,
                prompt: Prompt::V1,
                ai: "data".into(),
                resume: None,
            }))
        );
        assert!(matches!(
            parse(&args(&["record", "--dataset", "d", "--out", "o"])),
            Ok(Command::Record(RecordArgs { ai, prompt: Prompt::V1, split: None, .. }))
                if ai == std::path::Path::new("ai")
        ));
        assert!(matches!(
            parse(&args(&[
                "record", "--dataset", "d", "--out", "o", "--split", "dev", "--prompt", "v2",
                "--resume", "r.json"
            ])),
            Ok(Command::Record(RecordArgs { split: Some(s), prompt: Prompt::V2, resume: Some(_), .. }))
                if s == "dev"
        ));
        for bad in [
            &["record"][..],
            &["record", "--dataset", "d"],
            &["record", "--out", "o"],
            &["record", "--dataset", "d", "--out"],
            &["record", "--dataset", "d", "--out", "o", "--cassette", "c"],
            &["record", "--dataset", "d", "--out", "o", "--gate", "full"],
            &["record", "--dataset", "d", "--out", "o", "--force"],
            &["record", "--dataset", "d", "--out", "o", "--prompt", "V2"],
            &["record", "--dataset", "d", "--out", "o", "--prompt"],
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
