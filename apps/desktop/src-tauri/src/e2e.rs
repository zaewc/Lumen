//! The desktop flow end to end (roadmap 11.12): the real invoke handler,
//! capability and commands on Tauri's mock runtime, against a fixture home.
//!
//! The app's data folder and the quarantine store both follow `HOME`, so the
//! test re-runs itself as a child process with `HOME` pointing at a fixture.
//! Nothing outside the fixture is touched, and no environment variable is
//! changed inside a running process.

use std::path::Path;
use std::process::Command;

use lumen_testkit::{Fixture, Node};
use serde_json::{Value, json};
use tauri::ipc::{CallbackFn, InvokeBody};
use tauri::test::{INVOKE_KEY, MockRuntime, get_ipc_response, mock_builder};
use tauri::webview::InvokeRequest;
use tauri::{AppHandle, Manager as _, RunEvent, WebviewWindow};

/// Set in the child process that runs the flow.
const CHILD: &str = "LUMEN_DESKTOP_E2E_HOME";

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

/// The flow itself, in the child: scan → plan → execute → held → restore →
/// plan → execute → finalize, plus a command the capability does not grant.
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
    let item = home.join("Library/Developer/Xcode/DerivedData/App-abc");
    let cycle = || -> Option<String> {
        let scan = ok(
            "scan",
            json!({ "request": { "roots": [home] }, "progress": "__CHANNEL__:7" }),
        );
        assert_eq!(scan["roots"][0]["coverage_complete"], true, "{scan}");
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
    assert!(!item.exists());
    assert_eq!(ok("held", json!({})), json!([]));
}

#[test]
fn the_desktop_flow_runs_end_to_end() {
    if let Some(home) = std::env::var_os(CHILD) {
        // The app sets itself up (state, window) when its event loop is ready.
        let app = crate::app(mock_builder())
            .build(crate::context())
            .unwrap_or_else(|e| unreachable!("{e}"));
        let exit = app.run_return(move |handle, event| {
            if matches!(event, RunEvent::Ready) {
                flow(handle, Path::new(&home));
                // Closing the last window ends the (mock) event loop.
                if let Some(window) = handle.get_webview_window("main") {
                    window.close().unwrap_or_else(|e| unreachable!("{e}"));
                }
            }
        });
        assert_eq!(exit, 0);
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
    let exe = std::env::current_exe().unwrap_or_else(|e| unreachable!("{e}"));
    let out = Command::new(exe)
        .args([
            "--exact",
            "e2e::the_desktop_flow_runs_end_to_end",
            "--nocapture",
        ])
        .env("HOME", &home)
        .env(CHILD, &home)
        .output()
        .unwrap_or_else(|e| unreachable!("{e}"));
    assert!(
        out.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}
