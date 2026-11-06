//! Reassessment end to end (ADR-0058): a real scan and assessment of a fixture
//! project, then the same item checked again after what kept it changed.

#![cfg(unix)]

use std::fs::File;

use lumen_app::reassess::{Outcome, reassess};
use lumen_app::{SystemClock, UuidV7Ids, raw_path};
use lumen_application::CancelToken;
use lumen_application::ports::NoopJudge;
use lumen_testkit::{Fixture, Node};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

fn rules(item: &lumen_application::assess::Assessed) -> Vec<String> {
    item.decision
        .fired_rules()
        .iter()
        .map(|fired| fired.rule.as_str().to_owned())
        .collect()
}

#[test]
fn an_item_kept_while_open_is_decided_again_once_closed_and_nothing_moves() -> Result<()> {
    let fx = Fixture::build(&[
        Node::File {
            path: "projects/app/Cargo.toml".into(),
            contents: b"[package]\nname = \"app\"\n".to_vec(),
        },
        Node::File {
            path: "projects/app/target/debug/deps/libapp.rlib".into(),
            contents: vec![7; 8192],
        },
    ])?;
    let db = fx.path("index.db")?;
    let root = std::fs::canonicalize(fx.root())?;
    let scan = lumen_app::scan(
        &[root.join("projects")],
        Some(&db),
        &SystemClock,
        &UuidV7Ids,
        &CancelToken::new(),
        &mut |_| {},
    )?;
    assert!(scan.stopped.is_none());
    let deps = raw_path(&root.join("projects/app/target/debug/deps"))?;
    let library = root.join("projects/app/target/debug/deps/libapp.rlib");

    let open = File::open(&library)?;
    let assessed = lumen_app::assess(&db, &SystemClock, &NoopJudge, &NoopJudge)?;
    let before = assessed
        .into_iter()
        .find(|item| item.candidate.path == deps)
        .ok_or("the build output is a candidate")?;
    assert!(
        rules(&before).iter().any(|rule| rule == "protect.in-use"),
        "kept while open: {:?}",
        rules(&before)
    );

    drop(open);
    let result = reassess(
        &db,
        std::slice::from_ref(&before),
        &SystemClock,
        &CancelToken::new(),
    )?;
    assert!(!result.cancelled);
    let [item] = result.items.as_slice() else {
        return Err("one item reassessed".into());
    };
    let Outcome::Decided { after, added } = &item.outcome else {
        return Err(format!("decided again: {item:?}").into());
    };
    assert!(
        !rules(after).iter().any(|rule| rule == "protect.in-use"),
        "no longer in use: {:?}",
        rules(after)
    );
    assert!(!added.is_empty(), "the fresh answer is new evidence");
    assert_eq!(std::fs::read(&library)?, vec![7; 8192], "nothing moves");

    let cancel = CancelToken::new();
    cancel.cancel();
    let stopped = reassess(&db, std::slice::from_ref(&before), &SystemClock, &cancel)?;
    assert!(stopped.cancelled && stopped.items.is_empty());
    Ok(())
}
