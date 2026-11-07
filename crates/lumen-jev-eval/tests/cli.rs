//! The `lumen-jev-eval` binary against the committed seed dataset, with
//! cassettes built from its own cases.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use lumen_jev_eval::{CASSETTE_SCHEMA, Case, SOFTWARE_CASSETTE_SCHEMA, load, load_software};
use lumen_testkit::Fixture;

const VERSION: &str = "2026.10.0";
const MODEL: &str = "claude-sonnet-5-5";

fn ai() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ai")
}

/// A recorded answer for `case`: `assessment` with high confidence.
fn answer(case: &Case, assessment: &str, reason: &str) -> serde_json::Value {
    let evidence = case
        .request
        .evidence
        .first()
        .map(|e| e.id.to_string())
        .unwrap_or_default();
    serde_json::json!({
        "case_id": case.case_id,
        "request_hash": case.request.hash(),
        "stop_reason": "end_turn",
        "model": MODEL,
        "body": serde_json::json!({
            "schema_version": "jev.judgment/1",
            "item_ref": case.request.item_ref,
            "assessment": assessment,
            "confidence": "high",
            "reason_codes": [reason],
            "evidence_refs": [evidence],
            "injection_suspected": false,
            "rationale": "Recorded for the CLI test.",
        })
        .to_string(),
    })
}

/// A cassette answering every case of the seed dataset with `assessment`.
fn cassette(fx: &Fixture, name: &str, assessment: &str, reason: &str) -> PathBuf {
    let dataset = load(&ai(), VERSION).unwrap_or_else(|e| unreachable!("{e}"));
    let entries: Vec<serde_json::Value> = dataset
        .golden
        .iter()
        .chain(&dataset.adversarial)
        .map(|case| answer(case, assessment, reason))
        .collect();
    let text = serde_json::json!({
        "schema": CASSETTE_SCHEMA,
        "judge": {
            "provider": "anthropic",
            "model": MODEL,
            "model_version": null,
            "prompt_version": "jev-judge/1",
        },
        "entries": entries,
    })
    .to_string();
    let path = fx.path(name).unwrap_or_else(|e| unreachable!("{e}"));
    std::fs::write(&path, text).unwrap_or_else(|e| unreachable!("{e}"));
    path
}

fn run(cassette: &Path, gate: &str) -> Output {
    Command::new(env!("CARGO_BIN_EXE_lumen-jev-eval"))
        .args(["run", "--dataset", VERSION, "--gate", gate, "--ai"])
        .arg(ai())
        .arg("--cassette")
        .arg(cassette)
        .output()
        .unwrap_or_else(|e| unreachable!("{e}"))
}

#[test]
fn a_cautious_judge_passes_the_observed_gate_but_not_the_release_gate() {
    let fx = Fixture::build(&[]).unwrap_or_else(|e| unreachable!("{e}"));
    let cautious = cassette(&fx, "cautious.json", "likely_needed", "owner_app_present");
    let observed = run(&cautious, "observed");
    let text = String::from_utf8_lossy(&observed.stdout);
    assert!(observed.status.success(), "{text}");
    assert!(text.contains("critical false positives 0/"), "{text}");
    assert!(text.contains("observed gate"), "{text}");
    // Seventeen cases cannot prove a 0.5% bound: the release gate fails.
    let full = run(&cautious, "full");
    assert_eq!(full.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&full.stdout).contains("release gate (full): FAIL"));
}

#[test]
fn a_reckless_judge_fails_every_gate() {
    let fx = Fixture::build(&[]).unwrap_or_else(|e| unreachable!("{e}"));
    let reckless = cassette(&fx, "reckless.json", "likely_removable", "owner_app_absent");
    let observed = run(&reckless, "observed");
    assert_eq!(
        observed.status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&observed.stdout)
    );
}

#[test]
fn bad_input_exits_with_status_2() {
    let fx = Fixture::build(&[]).unwrap_or_else(|e| unreachable!("{e}"));
    let missing = fx
        .path("missing.json")
        .unwrap_or_else(|e| unreachable!("{e}"));
    assert_eq!(run(&missing, "full").status.code(), Some(2));
    let usage = Command::new(env!("CARGO_BIN_EXE_lumen-jev-eval"))
        .arg("bogus")
        .output()
        .unwrap_or_else(|e| unreachable!("{e}"));
    assert_eq!(usage.status.code(), Some(2));
}

/// `record` refuses bad input before it reads the key or sends anything.
#[test]
fn record_checks_its_input_before_the_key() {
    let fx = Fixture::build(&[]).unwrap_or_else(|e| unreachable!("{e}"));
    let existing = cassette(&fx, "existing.json", "likely_needed", "owner_app_present");
    let before = std::fs::read(&existing).unwrap_or_else(|e| unreachable!("{e}"));
    let record = |dataset: &str, out: &Path| {
        Command::new(env!("CARGO_BIN_EXE_lumen-jev-eval"))
            .args(["record", "--dataset", dataset, "--ai"])
            .arg(ai())
            .arg("--out")
            .arg(out)
            .output()
            .unwrap_or_else(|e| unreachable!("{e}"))
    };
    let taken = record(VERSION, &existing);
    assert_eq!(taken.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&taken.stderr).contains("already exists"));
    assert_eq!(
        std::fs::read(&existing).unwrap_or_else(|e| unreachable!("{e}")),
        before,
        "never overwritten"
    );
    let fresh = fx.path("new.json").unwrap_or_else(|e| unreachable!("{e}"));
    assert_eq!(record("1999.01.0", &fresh).status.code(), Some(2));
    assert!(!fresh.exists(), "nothing written for bad input");
}

/// The committed Jeb recording replayed on one split, pinned to its prompt.
#[test]
fn a_split_is_replayed_on_its_own_with_the_recorded_prompt() {
    let jeb = ai().join("cassettes/jev-decisions/1/jeb-2026.10.1.json");
    let replay = |extra: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_lumen-jev-eval"))
            .args(["run", "--dataset", "2026.10.1", "--json", "--ai"])
            .arg(ai())
            .arg("--cassette")
            .arg(&jeb)
            .args(extra)
            .output()
            .unwrap_or_else(|e| unreachable!("{e}"))
    };
    let dev = replay(&["--split", "dev", "--prompt", "v1"]);
    assert_eq!(dev.status.code(), Some(1), "Jeb fails the gate");
    let report: serde_json::Value =
        serde_json::from_slice(&dev.stdout).unwrap_or_else(|e| unreachable!("{e}"));
    assert_eq!(report["split"], "dev");
    assert_eq!(report["golden"]["cases"], 346);
    assert_eq!(report["adversarial"]["cases"], 15);
    assert_eq!(report["golden"]["unanswered"], 0);
    let other_prompt = replay(&["--split", "dev", "--prompt", "v2"]);
    assert_eq!(other_prompt.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&other_prompt.stderr).contains("jev-decisions/1"));
    let no_split = replay(&["--split", "train"]);
    assert_eq!(no_split.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&no_split.stderr).contains("no split `train`"));

    // `record` refuses an unknown split before the key, and writes nothing.
    let fx = Fixture::build(&[]).unwrap_or_else(|e| unreachable!("{e}"));
    let out = fx.path("dev.json").unwrap_or_else(|e| unreachable!("{e}"));
    let record = Command::new(env!("CARGO_BIN_EXE_lumen-jev-eval"))
        .args([
            "record",
            "--dataset",
            "2026.10.1",
            "--split",
            "train",
            "--prompt",
            "v2",
        ])
        .arg("--ai")
        .arg(ai())
        .arg("--out")
        .arg(&out)
        .output()
        .unwrap_or_else(|e| unreachable!("{e}"));
    assert_eq!(record.status.code(), Some(2));
    assert!(!out.exists());
}

const SOFTWARE: &str = "software-2026.11.0";

/// A software answer in Jeb's wire format.
fn software_answer(purpose: &str, impact: &str, injection: f64) -> String {
    serde_json::json!({
        "model": "jeb",
        "answers": {
            "purpose": {"type": "choice", "choice": purpose, "probabilities": {purpose: 0.9}},
            "removal_impact": {"type": "choice", "choice": impact, "probabilities": {impact: 0.9}},
            "injection": {"type": "noul", "noul": injection},
        },
        "usage": {"input_tokens": 700, "output_tokens": 0},
    })
    .to_string()
}

fn label<T: serde::Serialize>(set: &std::collections::BTreeSet<T>) -> String {
    set.iter()
        .next()
        .and_then(|v| serde_json::to_value(v).ok())
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default()
}

/// A software cassette answering every case right, except that protected
/// cases are answered `impact` when it is given.
fn software_cassette(fx: &Fixture, name: &str, protected_impact: Option<&str>) -> PathBuf {
    let dataset = load_software(&ai(), SOFTWARE).unwrap_or_else(|e| unreachable!("{e}"));
    let entries: Vec<serde_json::Value> = dataset
        .cases
        .iter()
        .map(|case| {
            let impact = match protected_impact {
                Some(impact) if case.protected => impact.to_owned(),
                _ => label(&case.expected.removal_impact),
            };
            let injection = if case.adversarial { 0.9 } else { 0.0 };
            serde_json::json!({
                "case_id": case.case_id,
                "request_hash": case.request.hash(),
                "stop_reason": "end_turn",
                "model": "jeb",
                "body": software_answer(&label(&case.expected.purpose), &impact, injection),
            })
        })
        .collect();
    let text = serde_json::json!({
        "schema": SOFTWARE_CASSETTE_SCHEMA,
        "judge": {"provider": "gsm.jeb", "model": "jeb", "prompt_version": "jev-software/1"},
        "entries": entries,
    })
    .to_string();
    let path = fx.path(name).unwrap_or_else(|e| unreachable!("{e}"));
    std::fs::write(&path, text).unwrap_or_else(|e| unreachable!("{e}"));
    path
}

fn run_software(cassette: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_lumen-jev-eval"))
        .args(["run-software", "--dataset", SOFTWARE, "--ai"])
        .arg(ai())
        .arg("--cassette")
        .arg(cassette)
        .output()
        .unwrap_or_else(|e| unreachable!("{e}"))
}

#[test]
fn run_software_reports_the_model_and_gates_what_is_shown() {
    let fx = Fixture::build(&[]).unwrap_or_else(|e| unreachable!("{e}"));
    let right = run_software(&software_cassette(&fx, "right.json", None));
    let text = String::from_utf8_lossy(&right.stdout);
    assert!(right.status.success(), "{text}");
    assert!(
        text.contains("0 answered only_this_app by the model"),
        "{text}"
    );
    assert!(text.contains("software gate"), "{text}");
    assert!(text.contains(": PASS"), "{text}");

    // A model that says "only this app" of every protected case is reported,
    // but the guard turns those answers into unknown before anyone sees them.
    let reckless = run_software(&software_cassette(
        &fx,
        "reckless.json",
        Some("only_this_app"),
    ));
    let text = String::from_utf8_lossy(&reckless.stdout);
    assert!(reckless.status.success(), "{text}");
    assert!(
        text.contains("8 answered only_this_app by the model"),
        "{text}"
    );
    assert!(
        text.contains("0 protected or adversarial cases say only_this_app"),
        "{text}"
    );
    assert!(text.contains(": PASS"), "{text}");

    // A judgment cassette is bad input for `run-software`.
    let judgments = cassette(&fx, "judgments.json", "likely_needed", "owner_app_present");
    assert_eq!(run_software(&judgments).status.code(), Some(2));
}

/// `record-software` refuses bad input before it reads the key or sends
/// anything.
#[test]
fn record_software_checks_its_input_before_the_key() {
    let fx = Fixture::build(&[]).unwrap_or_else(|e| unreachable!("{e}"));
    let existing = software_cassette(&fx, "existing.json", None);
    let record = |dataset: &str, out: &Path| {
        Command::new(env!("CARGO_BIN_EXE_lumen-jev-eval"))
            .args(["record-software", "--dataset", dataset, "--ai"])
            .arg(ai())
            .arg("--out")
            .arg(out)
            .output()
            .unwrap_or_else(|e| unreachable!("{e}"))
    };
    let taken = record(SOFTWARE, &existing);
    assert_eq!(taken.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&taken.stderr).contains("already exists"));
    let fresh = fx.path("new.json").unwrap_or_else(|e| unreachable!("{e}"));
    // A judgment dataset is not a software dataset.
    assert_eq!(record(VERSION, &fresh).status.code(), Some(2));
    assert!(!fresh.exists(), "nothing written for bad input");
    let nowhere = fresh.with_file_name("missing").join("out.json");
    let no_folder = record(SOFTWARE, &nowhere);
    assert_eq!(no_folder.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&no_folder.stderr).contains("create it first"));
}
