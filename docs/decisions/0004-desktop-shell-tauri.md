# ADR-0004: Use Tauri 2 as the desktop shell with the core in the host process

- Status: Accepted
- Date: 2026-10-06

## Context and problem statement

macOS and Windows need a desktop application with a small memory footprint, a
native-feeling UI, strong security, and direct access to the Rust core. The UI should
be shared with the web dashboard where practical.

## Decision drivers

- Small footprint (no bundled browser engine).
- The webview must be treated as untrusted; the core must be trusted.
- Reuse of one React UI between desktop and the browser dashboard (ADR-0009).
- Fast path from UI to the Rust core without an extra process.
- Keep a native-UI option open for the future.

## Considered options

1. Tauri 2.12 (system WKWebView / WebView2) with the Rust core running in the trusted
   Tauri host process.
2. Native per-platform UIs: SwiftUI + UniFFI on macOS, WinUI 3 (Windows App SDK 2.x)
   + C# on Windows.
3. Electron.
4. Rust-native UI toolkits (Slint, egui, Dioxus).

## Decision outcome

Chosen option: **1, Tauri 2.12.x** (exact version pinned; Tauri 3 is alpha only).

- The core runs **in the Tauri host process**, not as a sidecar. The webview is
  already out-of-process and untrusted; a sidecar adds lifecycle bugs (Tauri's sidecar
  exit fix was reverted on 2026-09-25) without moving the trust boundary.
- The command API is the main safety control: the UI reads view models, requests
  plans by **selection IDs**, and executes plans by **plan ID plus a confirmation
  token**. No command accepts a raw filesystem path for mutation.
- Security configuration: per-window capabilities containing only Lumen's own
  commands, each defined as its own permission; `removeUnusedCommands`; strict CSP;
  isolation pattern; navigation denial; all webview permission requests denied. The
  `fs`, `shell` and `localhost` plugins and the asset protocol are not registered.
- Streaming uses `tauri::ipc::Channel<T>`; large pages use `tauri::ipc::Response`;
  events are used only for small notifications.
- A `lumen-ffi` (UniFFI) facade is maintained from the start for mobile, which keeps a
  later native SwiftUI or WinUI shell possible without touching the core.
- The panic strategy is `unwind` for hosts and the FFI library, so an adapter panic
  cannot abort the process mid-operation; crash safety still relies on the journal
  (ADR-0015).

### Consequences

- Good: one UI codebase; small bundles; a deny-by-default IPC surface.
- Good: no FFI hop between the desktop UI host and the core.
- Bad: WebKit vs Chromium rendering differences; dense graph views must be
  benchmarked early on both.
- Bad: Tauri provides no background-service or privileged-helper story; those are
  built with OS mechanisms (ADR-0005).
- Bad: `tauri-specta` is still a release candidate, so typed command bindings use the
  OpenAPI-generated client over an IPC transport, or exactly pinned versions
  (ADR-0011).

Rejected:

- Native UIs give the best fidelity but double UI cost, and the C# binding path lags
  UniFFI (`uniffi-bindgen-cs` targets 0.31).
- Electron ships its own Chromium, uses more memory, and adds a Node attack surface.
- Slint, egui and Dioxus offer no dashboard reuse and weaker data-visualisation and
  accessibility ecosystems. egui remains acceptable for internal developer tools.

## More information

- [Desktop architecture research](../research/01-desktop-architecture.md)
- [Web and monorepo research](../research/07-web-and-monorepo.md)
- ADR-0005, ADR-0009, ADR-0023
