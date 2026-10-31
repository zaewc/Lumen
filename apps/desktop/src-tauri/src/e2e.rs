//! The desktop flow end to end (roadmap 11.12): the real invoke handler,
//! capability and commands on Tauri's mock runtime, against a fixture home.
//!
//! The app's data folder and the quarantine store both follow `HOME`, so the
//! test re-runs itself as a child process with `HOME` pointing at a fixture.
//! Nothing outside the fixture is touched, and no environment variable is
//! changed inside a running process. Judge keys go to an in-memory secret
//! store, never the keychain, and AI review is off while anything is
//! assessed, so nothing is sent anywhere.

use std::path::Path;
use std::process::Command;
use std::sync::Arc;

use lumen_testkit::{Fixture, MemorySecretStore, Node};
use serde_json::{Value, json};
use tauri::ipc::{CallbackFn, InvokeBody};
use tauri::test::{INVOKE_KEY, MockRuntime, get_ipc_response, mock_builder};
use tauri::webview::InvokeRequest;
use tauri::{AppHandle, Manager as _, RunEvent, WebviewWindow};

/// Set in the child process that runs the flow.
const CHILD: &str = "LUMEN_DESKTOP_E2E_HOME";

/// Set in the child process that starts after an interrupted cleanup.
const RECOVERY: &str = "LUMEN_DESKTOP_RECOVERY_HOME";

/// Re-runs `test` in a child process with `HOME` (and `var`) at `home`.
fn in_child(test: &str, var: &str, home: &Path) {
    let exe = std::env::current_exe().unwrap_or_else(|e| unreachable!("{e}"));
    let out = Command::new(exe)
        .args(["--exact", test, "--nocapture"])
        .env("HOME", home)
        .env(var, home)
        .output()
        .unwrap_or_else(|e| unreachable!("{e}"));
    assert!(
        out.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

/// Runs `flow` once the app, built as the real one is, has set itself up.
fn run_app(flow: impl Fn(&AppHandle<MockRuntime>) + 'static) {
    let secrets = Arc::new(MemorySecretStore::default());
    let app = crate::app(mock_builder(), Some(secrets), None)
        .build(crate::context())
        .unwrap_or_else(|e| unreachable!("{e}"));
    let exit = app.run_return(move |handle, event| {
        if matches!(event, RunEvent::Ready) {
            flow(handle);
            // Closing the last window ends the (mock) event loop.
            if let Some(window) = handle.get_webview_window("main") {
                window.close().unwrap_or_else(|e| unreachable!("{e}"));
            }
        }
    });
    assert_eq!(exit, 0);
}

fn invoke(window: &WebviewWindow<MockRuntime>, cmd: &str, body: Value) -> Result<Value, Value> {
    get_ipc_response(
        window,
        InvokeRequest {
            cmd: cmd.into(),
            callback: CallbackFn(0),
            error: CallbackFn(1),
            url: "tauri://localhost"
                .parse()
                .unwrap_or_else(|e| unreachable!("{e}")),
            body: InvokeBody::Json(body),
            headers: tauri::http::HeaderMap::default(),
            invoke_key: INVOKE_KEY.to_owned(),
        },
    )
    .map(|response| {
        response
            .deserialize::<Value>()
            .unwrap_or_else(|e| unreachable!("{e}"))
    })
}

/// AI review: not chosen and off at first, without a key (ADR-0050), a key
/// stored (never shown), on with the time of consent, then off again.
fn ai_review(window: &WebviewWindow<MockRuntime>) {
    let ok = |cmd: &str, body: Value| {
        invoke(window, cmd, body).unwrap_or_else(|e| unreachable!("{cmd}: {e}"))
    };
    assert_eq!(
        ok("ai_settings", json!({})),
        json!({
            "chosen": false, "enabled": false, "consented_at": null, "key": "none",
            "key_source": null
        })
    );
    let disk = ok("disk", json!({}));
    let bytes = |field: &str| {
        disk[field]
            .as_str()
            .and_then(|b| b.parse::<u64>().ok())
            .unwrap_or_default()
    };
    assert!(
        bytes("total_bytes") > 0 && bytes("available_bytes") <= bytes("total_bytes"),
        "{disk}"
    );
    assert!(
        invoke(window, "set_ai_review", json!({ "enabled": true })).is_err(),
        "not without a key"
    );
    let rejected = invoke(window, "set_ai_key", json!({ "key": "not a key" }))
        .err()
        .unwrap_or_else(|| unreachable!("a key has no spaces"));
    assert!(!rejected.to_string().contains("not a key"), "{rejected}");
    let stored = ok("set_ai_key", json!({ "key": "jeb-e2e-fake-key" }));
    assert_eq!(stored["key"], "jeb", "{stored}");
    assert_eq!(stored["key_source"], "stored", "{stored}");
    assert!(!stored.to_string().contains("jeb-e2e-fake-key"), "{stored}");
    let on = ok("set_ai_review", json!({ "enabled": true }));
    assert_eq!(on["chosen"], true, "{on}");
    assert_eq!(on["enabled"], true, "{on}");
    assert!(on["consented_at"].is_string(), "{on}");
    let off = ok("set_ai_review", json!({ "enabled": false }));
    assert_eq!(off["enabled"], false, "{off}");
    assert_eq!(
        off["consented_at"], on["consented_at"],
        "the consent stays on record"
    );
    assert_eq!(ok("ai_settings", json!({})), off);
}

/// The flow itself, in the child: AI review's switch, then scan → plan →
/// execute → held → restore → plan → execute → finalize, plus a command the
/// capability does not grant.
fn flow(app: &AppHandle<MockRuntime>, home: &Path) {
    let window = app
        .get_webview_window("main")
        .unwrap_or_else(|| unreachable!("the main window"));
    let ok = |cmd: &str, body: Value| {
        invoke(&window, cmd, body).unwrap_or_else(|e| unreachable!("{cmd}: {e}"))
    };

    assert!(
        invoke(&window, "plugin:fs|read_dir", json!({})).is_err(),
        "nothing outside Lumen's own commands is reachable"
    );
    ai_review(&window);
    assert!(
        invoke(
            &window,
            "run_tool_command",
            json!({ "entry": "docker-desktop-disk-image" })
        )
        .is_err(),
        "no command runs for an item the latest assessment did not find (ADR-0049)"
    );
    let item = home.join("Library/Developer/Xcode/DerivedData/App-abc");
    let cycle = || -> Option<String> {
        let scan = ok(
            "scan",
            json!({ "request": { "roots": [home] }, "progress": "__CHANNEL__:7" }),
        );
        assert_eq!(scan["roots"][0]["coverage_complete"], true, "{scan}");
        let assessed = ok("assess", json!({ "progress": "__CHANNEL__:8" }));
        assert!(
            assessed.as_array().is_some_and(|items| !items.is_empty()),
            "{assessed}"
        );
        assert!(
            item.exists(),
            "assessment only reads, even without a progress listener"
        );
        // REVIEW instead of QUARANTINE (in-use visibility): covered elsewhere.
        let plan = invoke(&window, "plan", json!({})).ok()?;
        let report = ok(
            "execute",
            json!({ "request": { "plan": plan["plan"], "confirmation": plan["hash"] } }),
        );
        let result = &report["items"][0]["result"];
        (result["kind"] == "quarantined")
            .then(|| result["operation"].as_str().unwrap_or_default().to_owned())
    };
    let Some(first) = cycle() else {
        return;
    };
    assert!(!item.exists(), "moved out of its place");
    let held = ok("held", json!({}));
    assert_eq!(held[0]["operation"], first.as_str(), "{held}");
    assert!(
        held[0]["reclaim_bytes"]
            .as_str()
            .is_some_and(|bytes| bytes.parse::<u64>().is_ok_and(|b| b >= 4096)),
        "the plan's estimate is recorded with the item: {held}"
    );
    assert_eq!(held[0]["chosen_by"], "policy", "{held}");
    let restored = ok("restore", json!({ "operationId": first }));
    assert_eq!(restored["outcome"]["kind"], "restored", "{restored}");
    assert_eq!(
        std::fs::read(item.join("Build/out.o")).unwrap_or_default(),
        vec![7; 4096],
        "back in place, intact"
    );
    let Some(second) = cycle() else {
        return;
    };
    let finalized = ok("finalize", json!({ "operationId": second }));
    assert_eq!(finalized["outcome"]["kind"], "finalized", "{finalized}");
    let measured = &finalized["space_change"];
    assert!(
        measured["volume"]
            .as_str()
            .is_some_and(|v| v.starts_with("dev:")),
        "{finalized}"
    );
    for key in ["before_available_bytes", "after_available_bytes"] {
        assert!(
            measured[key]
                .as_str()
                .is_some_and(|v| v.parse::<u64>().is_ok()),
            "{finalized}"
        );
    }
    assert!(!item.exists());
    assert_eq!(ok("held", json!({})), json!([]));
}

#[test]
fn the_desktop_flow_runs_end_to_end() {
    if let Some(home) = std::env::var_os(CHILD) {
        run_app(move |handle| flow(handle, Path::new(&home)));
        return;
    }
    let fx = Fixture::build(&[
        Node::File {
            path: "home/Library/Developer/Xcode/DerivedData/App-abc/Build/out.o".into(),
            contents: vec![7; 4096],
        },
        Node::Dir {
            path: "home/Library/Application Support".into(),
        },
    ])
    .unwrap_or_else(|e| unreachable!("{e}"));
    let home = std::fs::canonicalize(fx.path("home").unwrap_or_else(|e| unreachable!("{e}")))
        .unwrap_or_else(|e| unreachable!("{e}"));
    in_child("e2e::the_desktop_flow_runs_end_to_end", CHILD, &home);
}

/// The operation a crash interrupted in the fixture home.
const INTERRUPTED: &str = "0190f0d2-6c1e-7a3b-8c4d-5e6f70819401";

/// What a crash right after the move leaves: `Caches/big` in the store, and
/// only the intent in the app's ledger.
fn crash_after_move(home: &Path) {
    use lumen_application::ports::QuarantineExecutor as _;
    use lumen_domain::{OperationId, QuarantineEvent, Timestamp};
    use lumen_store_sqlite::{JournalEntry, LedgerStore, open_ledger};

    let support = home.join("Library/Application Support");
    let executor =
        lumen_fs_exec::Executor::new(&support, vec![b"Lumen".to_vec(), b"Quarantine".to_vec()]);
    let root = lumen_app::raw_path(home).unwrap_or_else(|e| unreachable!("{e}"));
    let item = executor
        .prepare(&root, &[b"Caches".to_vec(), b"big".to_vec()])
        .unwrap_or_else(|e| unreachable!("{e:?}"));
    let operation: OperationId = INTERRUPTED.parse().unwrap_or_else(|e| unreachable!("{e}"));
    let data = support.join(&crate::context::<MockRuntime>().config().identifier);
    std::fs::create_dir_all(&data).unwrap_or_else(|e| unreachable!("{e}"));
    let mut ledger = LedgerStore::new(
        open_ledger(&data.join("ledger.db")).unwrap_or_else(|e| unreachable!("{e}")),
    );
    ledger
        .journal(&JournalEntry {
            operation,
            event: QuarantineEvent::Journal,
            at: Timestamp::UNIX_EPOCH,
            detail: serde_json::from_slice(&item.0).unwrap_or_else(|e| unreachable!("{e}")),
        })
        .unwrap_or_else(|e| unreachable!("{e}"));
    executor
        .quarantine(&item, operation, b"{}")
        .unwrap_or_else(|e| unreachable!("{e:?}"));
}

/// UX-3: after a crash mid-cleanup, the next launch lists the moved item
/// without another cleanup, and it can be restored.
#[test]
fn an_interrupted_cleanup_is_listed_after_the_next_launch() {
    if let Some(home) = std::env::var_os(RECOVERY) {
        run_app(move |handle| {
            let window = handle
                .get_webview_window("main")
                .unwrap_or_else(|| unreachable!("the main window"));
            let item = Path::new(&home).join("Caches/big");
            let held = invoke(&window, "held", json!({})).unwrap_or_else(|e| unreachable!("{e}"));
            assert_eq!(held[0]["operation"], INTERRUPTED, "{held}");
            assert_eq!(held[0]["state"], "held_after_failure", "{held}");
            assert_eq!(held[0]["original"], item.display().to_string(), "{held}");
            let restored = invoke(&window, "restore", json!({ "operationId": INTERRUPTED }))
                .unwrap_or_else(|e| unreachable!("{e}"));
            assert_eq!(restored["outcome"]["kind"], "restored", "{restored}");
            assert_eq!(std::fs::read(&item).unwrap_or_default(), vec![7; 4096]);
        });
        return;
    }
    let fx = Fixture::build(&[
        Node::File {
            path: "home/Caches/big".into(),
            contents: vec![7; 4096],
        },
        Node::Dir {
            path: "home/Library/Application Support".into(),
        },
    ])
    .unwrap_or_else(|e| unreachable!("{e}"));
    let home = std::fs::canonicalize(fx.path("home").unwrap_or_else(|e| unreachable!("{e}")))
        .unwrap_or_else(|e| unreachable!("{e}"));
    crash_after_move(&home);
    assert!(
        !home.join("Caches/big").exists(),
        "moved by the crashed run"
    );
    in_child(
        "e2e::an_interrupted_cleanup_is_listed_after_the_next_launch",
        RECOVERY,
        &home,
    );
}
