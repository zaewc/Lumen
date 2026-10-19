//! `lumen-jev-eval`: Jev's evaluation gate from the command line (roadmap
//! 12.1, docs/ai/harness.md).
//!
//! ```text
//! lumen-jev-eval run --dataset <VERSION> --cassette <FILE> [--ai <DIR>]
//!                    [--gate full|observed] [--json]
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

// A command-line tool reports to the terminal by design.
#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::error::Error;
use std::path::PathBuf;
use std::process::ExitCode;

use lumen_jev_eval::{
    CRITICAL_FPR_TARGET, Cassette, DatasetReport, ReplayJudge, Report, load, run_dataset,
};

const USAGE: &str = "usage: lumen-jev-eval run --dataset <VERSION> --cassette <FILE> [--ai <DIR>] [--gate full|observed] [--json]";

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

fn parse(args: &[String]) -> Result<RunArgs, String> {
    let Some((command, rest)) = args.split_first() else {
        return Err("a command is needed".into());
    };
    if command != "run" {
        return Err(format!("unknown command `{command}`"));
    }
    let (mut dataset, mut cassette, mut ai, mut json) = (None, None, PathBuf::from("ai"), false);
    let mut gate = Gate::Full;
    let mut iter = rest.iter();
    while let Some(arg) = iter.next() {
        let mut value = || {
            iter.next()
                .cloned()
                .ok_or_else(|| format!("{arg} needs a value"))
        };
        match arg.as_str() {
            "--dataset" => dataset = Some(value()?),
            "--cassette" => cassette = Some(PathBuf::from(value()?)),
            "--ai" => ai = PathBuf::from(value()?),
            "--gate" => {
                gate = match value()?.as_str() {
                    "full" => Gate::Full,
                    "observed" => Gate::Observed,
                    other => return Err(format!("unknown gate `{other}`")),
                };
            }
            "--json" => json = true,
            other => return Err(format!("unexpected argument `{other}`")),
        }
    }
    Ok(RunArgs {
        dataset: dataset.ok_or("--dataset is needed")?,
        cassette: cassette.ok_or("--cassette is needed")?,
        ai,
        gate,
        json,
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

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let parsed = match parse(&args) {
        Ok(parsed) => parsed,
        Err(message) => {
            eprintln!("error: {message}\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    match run(&parsed) {
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
            Ok(RunArgs {
                dataset: "2026.10.0".into(),
                cassette: "c.json".into(),
                ai: "ai".into(),
                gate: Gate::Full,
                json: true,
            })
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
            Ok(RunArgs {
                gate: Gate::Observed,
                ..
            })
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
        assert!(parse(&args(&["record"])).is_err());
        assert!(parse(&args(&["run", "--cassette"])).is_err());
        assert!(parse(&[]).is_err());
    }
}
