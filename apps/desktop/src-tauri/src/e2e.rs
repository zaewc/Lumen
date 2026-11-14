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
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

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

/// Set in the child process that tries to quit and update mid-cleanup.
const HOLD: &str = "LUMEN_DESKTOP_HOLD_HOME";

/// The answer the host's confirmation dialog gets in the child (FS-1).
static ANSWER: AtomicBool = AtomicBool::new(true);

/// How often the host asked.
static ASKED: AtomicUsize = AtomicUsize::new(0);

/// The app as the real one is built, with [`ANSWER`] for its dialogs.
fn app_under_test() -> tauri::Builder<MockRuntime> {
    let secrets = Arc::new(MemorySecretStore::default());
    crate::app_with_confirmation(
        mock_builder(),
        Some(secrets),
        None,
        Arc::new(|_: &str, _: &str, _: &str, _: &str| {
            ASKED.fetch_add(1, Ordering::SeqCst);
            ANSWER.load(Ordering::SeqCst)
        }),
    )
}

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
    let app = app_under_test()
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

/// FS-1: declining the host's own dialog leaves `plan` unexecuted and `item`
/// in place, however the page called `execute`.
fn declining_moves_nothing(window: &WebviewWindow<MockRuntime>, plan: &Value, item: &Path) {
    ANSWER.store(false, Ordering::SeqCst);
    let declined = invoke(
        window,
        "execute",
        json!({ "request": { "plan": plan["plan"], "confirmation": plan["hash"] } }),
    );
    ANSWER.store(true, Ordering::SeqCst);
    assert!(declined.is_err(), "declined in the host's own dialog");
    assert!(item.exists(), "declining moves nothing");
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
        let excluded_store = lumen_app::store::scan_exclusion_paths()
            .unwrap_or_else(|e| unreachable!("{e}"))
            .iter()
            .any(|path| path.starts_with(home) && path.is_dir());
        let scan = ok(
            "scan",
            json!({ "request": { "roots": [home] }, "progress": "__CHANNEL__:7" }),
        );
        assert_eq!(
            scan["roots"][0]["coverage_complete"], !excluded_store,
            "{scan}"
        );
        let assessed = ok("assess", json!({ "progress": "__CHANNEL__:8" }));
        assert!(
            assessed.as_array().is_some_and(|items| !items.is_empty()),
            "{assessed}"
        );
        assert!(!assessed.to_string().contains("/Quarantine/"), "{assessed}");
        assert!(
            item.exists(),
            "assessment only reads, even without a progress listener"
        );
        // REVIEW instead of QUARANTINE (in-use visibility): covered elsewhere.
        let plan = invoke(&window, "plan", json!({})).ok()?;
        declining_moves_nothing(&window, &plan, &item);
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
        // FS-1: before each execute (declined, then confirmed, twice) and the finalize.
        assert_eq!(ASKED.load(Ordering::SeqCst), 5, "the host asked each time");
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

/// The child's half of [`quit_and_updates_wait_for_a_cleanup`]: while an
/// item action runs, the update and closing the window wait; while an update
/// installs, no item action starts; and once both are done the window closes.
fn hold_quit_and_updates() {
    let app = app_under_test()
        .build(crate::context())
        .unwrap_or_else(|e| unreachable!("{e}"));
    let mut moving = None;
    let mut held = 0;
    let exit = app.run_return(move |handle, event| {
        crate::hold_quit_events(handle, &event);
        let window = || {
            handle
                .get_webview_window("main")
                .unwrap_or_else(|| unreachable!("the main window"))
        };
        let activity = Arc::clone(&handle.state::<crate::commands::Core>().activity);
        match event {
            RunEvent::Ready => {
                moving = Some(activity.moving().unwrap_or_else(|e| unreachable!("{e}")));
                let refused = invoke(&window(), "install_update", json!({}))
                    .err()
                    .unwrap_or_else(|| unreachable!("no update while items move"));
                assert!(refused.to_string().contains("moving items"), "{refused}");
                window().close().unwrap_or_else(|e| unreachable!("{e}"));
            }
            // The close request above has been answered by now.
            RunEvent::MainEventsCleared if moving.is_some() => {
                held += 1;
                moving = None;
                let updating = activity.updating().unwrap_or_else(|e| unreachable!("{e}"));
                for (cmd, body) in [
                    (
                        "execute",
                        json!({ "request": { "plan": "x", "confirmation": "x" } }),
                    ),
                    ("restore", json!({ "operationId": INTERRUPTED })),
                    ("finalize", json!({ "operationId": INTERRUPTED })),
                ] {
                    let refused = invoke(&window(), cmd, body)
                        .err()
                        .unwrap_or_else(|| unreachable!("{cmd} waits for the update"));
                    assert!(
                        refused.to_string().contains("installing an update"),
                        "{refused}"
                    );
                }
                drop(updating);
                window().close().unwrap_or_else(|e| unreachable!("{e}"));
            }
            // The mock runtime asks to exit once it has closed the last window.
            RunEvent::ExitRequested { .. } => {
                assert!(moving.is_none(), "the window closed while items moved");
            }
            RunEvent::Exit => assert_eq!(held, 1),
            _ => {}
        }
    });
    assert_eq!(exit, 0);
}

/// UX-3: quitting and updating wait for a clean-up or Quarantine action.
#[test]
fn quit_and_updates_wait_for_a_cleanup() {
    if std::env::var_os(HOLD).is_some() {
        hold_quit_and_updates();
        return;
    }
    let fx = Fixture::build(&[Node::Dir {
        path: "home/Library/Application Support".into(),
    }])
    .unwrap_or_else(|e| unreachable!("{e}"));
    let home = std::fs::canonicalize(fx.path("home").unwrap_or_else(|e| unreachable!("{e}")))
        .unwrap_or_else(|e| unreachable!("{e}"));
    in_child("e2e::quit_and_updates_wait_for_a_cleanup", HOLD, &home);
}
