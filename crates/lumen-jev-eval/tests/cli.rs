//! The `lumen-jev-eval` binary against the committed seed dataset, with
//! cassettes built from its own cases.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use lumen_jev_eval::{CASSETTE_SCHEMA, Case, load};
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
