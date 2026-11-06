//! Cleanup effectiveness on a labeled home (issue #703): the real `lumen scan`
//! and `lumen assess` on a fixture whose paths say what should happen to them.
//! No labeled user data may be in, or contain, a quarantine proposal; every
//! labeled artifact must be found. Timings are not gated here: shared runners
//! are too noisy (docs/architecture/performance.md).

#![cfg(target_os = "macos")]

use std::process::Command;

use lumen_testkit::Fixture;
use lumen_testkit::labeled::{Candidate, Expect, macos_home, score};

#[test]
fn a_labeled_home_gets_no_unsafe_proposal_and_every_artifact_is_found()
-> Result<(), Box<dyn std::error::Error>> {
    let labeled = macos_home();
    let fx = Fixture::build(&labeled.nodes)?;
    let home = std::fs::canonicalize(fx.path("home")?)?;
    let db = fx.path("index.db")?;
    // Held open for the whole assessment, as an editor or build would.
    let _open = labeled
        .labels
        .iter()
        .filter(|label| label.expect == Expect::InUse)
        .map(|label| std::fs::File::open(home.join(label.path)))
        .collect::<Result<Vec<_>, _>>()?;
    let run = |args: &[&std::ffi::OsStr]| {
        Command::new(env!("CARGO_BIN_EXE_lumen"))
            .args(args)
            .env("HOME", &home)
            .output()
    };
    let scan = run(&[
        "scan".as_ref(),
        home.as_os_str(),
        "--db".as_ref(),
        db.as_os_str(),
    ])?;
    assert!(
        scan.status.success(),
        "{}",
        String::from_utf8_lossy(&scan.stderr)
    );
    let out = run(&[
        "assess".as_ref(),
        "--db".as_ref(),
        db.as_os_str(),
        "--json".as_ref(),
    ])?;
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&out.stdout)?;
    let candidates: Vec<Candidate> = report["candidates"]
        .as_array()
        .ok_or("no candidates")?
        .iter()
        .map(|c| Candidate {
            path: c["path"].as_str().unwrap_or_default().to_owned(),
            verdict: c["verdict"].as_str().unwrap_or_default().to_owned(),
            reclaimable_bytes: c["reclaimable_bytes"].as_u64().unwrap_or_default(),
        })
        .collect();
    let home = home.to_str().ok_or("home is not UTF-8")?;
    let result = score(home, &labeled.labels, &candidates);
    assert_eq!(
        result.unsafe_recommendations,
        Vec::<&str>::new(),
        "{candidates:#?}"
    );
    assert_eq!(
        result.proposed_missed,
        Vec::<&str>::new(),
        "{candidates:#?}"
    );
    assert_eq!(result.review_missed, Vec::<&str>::new(), "{candidates:#?}");
    assert!(result.proposed_bytes > 0);
    Ok(())
}
