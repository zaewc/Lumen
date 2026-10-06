# Desktop Architecture for Lumen: Tauri 2 Shell, Shared Rust Core, Least-Privilege Helpers

> Researched: 2026-10-05 · Scope: macOS + Windows desktop shell options, Rust-core FFI strategy, process/privilege architecture, and a recommended desktop architecture for Lumen.

## Summary

- **Tauri 2 is the current stable line: `tauri` 2.12.1 (crate + `@tauri-apps/cli` / `@tauri-apps/api` 2.12.1, published 2026-09-30/10-01).** Tauri 3 exists only as alphas (`tauri` 3.0.0-alpha.4, `tauri-runtime-cef` 3.0.0-alpha.5). Do not build on v3 yet. ([crates.io](https://crates.io/crates/tauri), [Tauri 2.12 blog](https://v2.tauri.app/blog/tauri-2.12/))
- **Tauri 2.12 dropped Windows 7 and set MSRV to Rust 1.90**, with a "stable − 3" MSRV policy. It also adds `on_permission_request` for webview permission prompts. Some older Tauri docs still say Windows 7 is supported. Treat 2.12 as the authoritative source. ([Tauri 2.12](https://v2.tauri.app/blog/tauri-2.12/))
- **Tauri's security model fits Lumen well.** The Rust host is trusted, and the webview is untrusted and runs out-of-process. The webview reaches Rust only through **capabilities → permissions → scopes**, a compile-time CSP with nonces and hashes, an optional **isolation pattern** (AES-GCM-encrypted IPC through a sandboxed iframe), and `removeUnusedCommands` (2.4+). Capabilities explicitly do **not** protect against bad Rust code, overly broad scopes, or webview 0-days. ([Capabilities](https://v2.tauri.app/security/capabilities/), [Isolation](https://v2.tauri.app/concept/inter-process-communication/isolation/))
- **Tauri 2's IPC is good enough for Lumen's data volumes.** Use commands for request/response, `tauri::ipc::Channel<T>` for ordered streaming (scan progress, graph deltas), and `tauri::ipc::Response` for raw binary payloads. Use events only for small, low-rate notifications, because their payloads are always JSON strings. ([Calling Rust](https://v2.tauri.app/develop/calling-rust/), [Calling frontend](https://v2.tauri.app/develop/calling-frontend/))
- **Tauri has no first-class background service or privileged-helper story.** Sidecars run as child processes. Sidecar cleanup on exit is still the app's job: a framework fix (PR #14443) was merged 2026-09-18 and **reverted** 2026-09-25 because of a deadlock. A daemon or helper must be shipped and registered with OS mechanisms (macOS `SMAppService`, Windows SCM / Task Scheduler) through `bundle.macOS.files` and NSIS hooks. ([PR #14443](https://github.com/tauri-apps/tauri/pull/14443), [macOS bundle](https://v2.tauri.app/distribute/macos-application-bundle/), [Windows installer](https://v2.tauri.app/distribute/windows-installer/))
- **On macOS, `SMAppService` (macOS 13+) is the only supported way to register helpers.** `.agent(plistName:)` covers a per-user agent. `.daemon(plistName:)` covers a root daemon and **requires admin approval in System Settings**. `SMJobBless` has been deprecated since macOS 13 ("Please use SMAppService instead"). Use XPC with peer code-signing checks: `xpc_connection_set_peer_code_signing_requirement` on macOS 12+, or `XPCPeerRequirement` / `XPCListener(…requirement:…)` on macOS 26+. ([SMAppService](https://developer.apple.com/documentation/servicemanagement/smappservice), [SMJobBless](https://developer.apple.com/documentation/servicemanagement/smjobbless(_:_:_:_:)), [XPCPeerRequirement](https://developer.apple.com/documentation/xpc/xpcpeerrequirement))
- **Windows named pipes are unsafe by default for privileged IPC.** If `CreateNamedPipe` is passed NULL security attributes, the pipe gets a *default* security descriptor that grants **read to Everyone and anonymous** (a NULL *DACL*, which is different, grants everyone full access). A Lumen helper pipe must set an explicit DACL and `PIPE_REJECT_REMOTE_CLIENTS`, and it should use `FILE_FLAG_FIRST_PIPE_INSTANCE` to prevent pipe squatting. Do not grant clients `FILE_GENERIC_WRITE` in that DACL, because it includes `FILE_CREATE_PIPE_INSTANCE` and lets a client create server instances. ([Named pipe security](https://learn.microsoft.com/en-us/windows/win32/ipc/named-pipe-security-and-access-rights), [CreateNamedPipe](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-createnamedpipea))
- **UniFFI 0.32.2 (2026-09-23) is the right tool for Swift and Kotlin bindings.** It supports async (Swift `async`, Kotlin `suspend`), typed errors (`#[derive(uniffi::Error)]` maps to `throws` and exceptions), and foreign traits. It has **no built-in cancellation**, and its Swift 6 `Sendable` support for async is still incomplete (issue #2448). It is "a long way from 1.0", so pin the exact version. ([crates.io](https://crates.io/crates/uniffi), [UniFFI futures](https://mozilla.github.io/uniffi-rs/latest/futures.html), [Swift overview](https://mozilla.github.io/uniffi-rs/latest/swift/overview.html))
- **C# and React Native bindings come from third parties and lag UniFFI.** `uniffi-bindgen-cs` v0.11.0 targets UniFFI 0.31.0. `uniffi-bindgen-react-native` is at 0.31.0-6. This matters if Lumen picks a WinUI/C# UI or an Expo mobile app, because the core's UniFFI version is gated by the slowest binding generator. ([uniffi-bindgen-cs](https://github.com/NordSecurity/uniffi-bindgen-cs), [npm](https://www.npmjs.com/package/uniffi-bindgen-react-native))
- **Native per-platform UIs give the best platform fidelity but cost about 2× UI engineering.** That means SwiftUI + UniFFI on macOS and WinUI 3 / Windows App SDK **2.5.1** (2.0 GA 2026-04-29) + C# on Windows. Electron (44.5.1) is rejected by default because each app ships its own Chromium, uses far more RAM, and has a larger attack surface. Slint, egui and Dioxus are viable Rust-native UIs but are weaker fits for a data-dense, accessible, web-dashboard-sharing product. ([Windows App SDK channels](https://learn.microsoft.com/en-us/windows/apps/windows-app-sdk/release-channels))
- **Recommendation:** use a **Tauri 2.12 shell** (React + TypeScript, shared with the web dashboard) that runs the **Rust core in the trusted Tauri host process**. Expose a **narrow, plan-ID-based command API**: JS never sends raw paths to destructive commands. Add a **headless per-user agent**, built from the same core, for scheduled scans and quarantine expiry. Add a **separate, minimal, privileged helper only when system-scope cleanup is needed**: on macOS via `SMAppService.daemon` + XPC peer requirements, on Windows via on-demand UAC elevation or a hardened service. Keep a UniFFI facade crate from day one so a native SwiftUI or WinUI shell stays a possible later choice.
- **Distribution:** a scanner that needs broad, Full-Disk-Access-style file visibility conflicts with the App Sandbox that the Mac App Store requires. Plan for Developer ID + notarization outside the Mac App Store. ([App Sandbox](https://developer.apple.com/documentation/security/app-sandbox))

## Findings

### 1. Tauri 2: versions and release status (verified 2026-10-05)

| Component | Stable | Pre-release | Source |
|---|---|---|---|
| `tauri` (crate) | **2.12.1** | 3.0.0-alpha.4 (2026-10-01) | [crates.io/tauri](https://crates.io/crates/tauri) |
| `tauri-build` | 2.7.1 | 3.0.0-alpha.3 | crates.io |
| `@tauri-apps/cli` | **2.12.1** (2026-09-30) | 3.0.0-alpha.4 | [npm](https://www.npmjs.com/package/@tauri-apps/cli) |
| `@tauri-apps/api` | **2.12.1** | 3.0.0-alpha.2 | [npm](https://www.npmjs.com/package/@tauri-apps/api) |
| `wry` (webview abstraction) | 0.57.0 (2026-09-08) | — | crates.io |
| `tao` (windowing) | 0.37.1 (2026-09-26) | — | crates.io |
| `tauri-runtime-cef` | none | 3.0.0-alpha.5 | crates.io, [release](https://github.com/tauri-apps/tauri/releases/tag/tauri-runtime-cef-v3.0.0-alpha.4) |
| `tauri-plugin-updater` | 2.13.1 | 3.0.0-alpha.2 | crates.io |
| `tauri-plugin-shell` | 2.4.0 | 3.0.0-alpha.2 | crates.io |
| `tauri-plugin-single-instance` | 2.5.2 | — | crates.io |
| `tauri-plugin-autostart` | 2.7.0 | — | crates.io |
| `tauri-plugin-deep-link` / `-log` / `-fs` / `-dialog` / `-store` / `-notification` / `-window-state` | 2.6.1 / 2.10.0 / 2.6.0 / 2.8.1 / 2.5.0 / 2.5.1 / 2.5.0 | — | crates.io |
| `tauri-specta` (typed TS bindings) | 1.0.2 (v1 only) | **2.0.0-rc.25** (Tauri 2) | crates.io, [repo](https://github.com/specta-rs/tauri-specta) |

What [Tauri 2.12](https://v2.tauri.app/blog/tauri-2.12/) (2026-09-26) changed:
- Windows 7 support is dropped because of WebView2. MSRV rises to **1.90**, with a "(stable − 3)" policy.
- New `on_permission_request` handler for webview permission prompts (geolocation, notifications, …). Lumen should deny everything by default.
- Android templates move to Gradle 9 / Kotlin 2 with `targetSdk` 37. Plugin authors move ProGuard rules to `consumer-rules.pro`.
- Tauri 3 and the CEF runtime are not mentioned.

**Tauri 3 status:** v3 alphas decouple the runtime from the `tauri` crate. Apps pick `tauri-runtime-wry` or `tauri-runtime-cef` via `Builder::runtime(...)`. This adds an optional **Chromium Embedded Framework** runtime that bundles a consistent engine instead of the system webview (per [release notes surfaced by search](https://github.com/tauri-apps/tauri/releases); exact semantics are **(unverified)** against final docs). The v3 alpha also removes the `macos-private-api` feature because transparency no longer needs private APIs ([releases](https://github.com/tauri-apps/tauri/releases)). There is no published v3 timeline. Treat CEF as a future option for webview consistency, not a current dependency.

### 2. Tauri architecture

- Crates: `tauri` (the hub: config, script injection, updater), `tauri-runtime` (an abstraction over webview libraries), `tauri-runtime-wry`, `tauri-macros`, `tauri-utils` (config parsing, CSP injection), `tauri-build`, `tauri-codegen` (compile-time asset embedding). **TAO** handles windows and **WRY** handles webviews. Tauri is "not a VM". ([Architecture](https://v2.tauri.app/concept/architecture/))
- Webviews by platform ([Webview versions](https://v2.tauri.app/reference/webview-versions/)):
  - **macOS/iOS: WKWebView.** It updates only with the OS, so "unsupported macOS versions do not receive WebKit updates". The default `bundle.macOS.minimumSystemVersion` is 10.13 ([macOS bundle](https://v2.tauri.app/distribute/macos-application-bundle/)). Lumen should raise it to 13.0 so it can use `SMAppService` (see §8).
  - **Windows: WebView2 (Evergreen Chromium).** It is preinstalled on Windows 11, and the installer can bootstrap it elsewhere.
  - **Linux:** webkit2gtk. **Android:** the system WebView.
- Consequence: two engines (WebKit and Chromium) render the same UI, so the frontend must be tested on both. The web dashboard runs in arbitrary browsers anyway, so a shared component library tested cross-engine pays off twice.
- The webview's renderer already runs **out of process** (WebView2 and WKWebView content processes). The Tauri "core" process is the native Rust host. That is why Tauri's docs draw the trust boundary between Rust and the webview, not between processes ([Security](https://v2.tauri.app/security/)).

### 3. Tauri security model (capabilities, permissions, scopes, CSP, isolation)

**Trust boundaries.** Rust core code has full system access. Frontend code reaches the system only through IPC that capabilities mediate. Tauri relies on OS-provided webviews so security patches arrive with OS updates. ([Security](https://v2.tauri.app/security/))

**Capabilities** ([docs](https://v2.tauri.app/security/capabilities/)):
- These are JSON or TOML files in `src-tauri/capabilities/`. Every file in that directory is enabled by default unless `app.security.capabilities` lists specific ones.
- Fields: `identifier`, `description`, `windows` (labels, wildcards allowed), `webviews`, `permissions`, an optional `platforms` list (`macOS`, `windows`, …), and `remote.urls` to grant IPC to remote origins.
- When a window matches several capabilities, it gets the **union** of their permissions. Keep each window to a single capability to avoid accidental privilege merging.
- On Linux and Android, Tauri "is unable to distinguish between requests from an embedded `<iframe>` and the window itself". This is a reason never to embed third-party iframes.
- **Not protected:** malicious or insecure Rust code, overly permissive scopes, 0-days or unpatched 1-days in the system webview, supply-chain attacks.

```json
{
  "$schema": "../gen/schemas/desktop-schema.json",
  "identifier": "main",
  "windows": ["main"],
  "permissions": [
    "core:default",
    "allow-get-scan-snapshot",
    "allow-propose-cleanup-plan",
    "allow-execute-approved-plan"
  ]
}
```

**Permissions** ([docs](https://v2.tauri.app/security/permissions/)):
- Format: `<plugin>:<permission>`, e.g. `fs:default`, `core:window:allow-set-title`. Identifiers are lowercase ASCII, up to 116 characters.
- Permission files are TOML in `permissions/`. A plugin's `default` set is added automatically when you add the plugin with the CLI.
- **App commands can have their own permissions** via `tauri_build::Attributes::app_manifest` / `AppManifest` in `build.rs`. Lumen should use this so each of its commands is individually allow-listed.

**Scopes** ([docs](https://v2.tauri.app/security/scope/)):
- Allow and deny lists, where **deny always wins**. Scope types must be serde-serializable.
- **The command implementation must enforce the scope itself.** "Command developers need to ensure that there are no scope bypasses possible." Lumen should not rely on fs-plugin scopes for safety. It should not expose `tauri-plugin-fs` to the webview at all.

**CSP** ([docs](https://v2.tauri.app/security/csp/)):
- Set `app.security.csp`. Tauri injects nonces and hashes for bundled scripts and styles at compile time.
- Recommended directives include `default-src 'self' customprotocol: asset:` and `connect-src ipc: http://ipc.localhost`. Use `'wasm-unsafe-eval'` only if WASM is needed.
- A CSP applies only if it is configured. Avoid remote CDNs.
- Related keys ([config reference](https://v2.tauri.app/reference/config/)):
  - `devCsp`.
  - `freezePrototype`: freezes `Object.prototype` against prototype pollution.
  - `dangerousDisableAssetCspModification`: keep it `false`.
  - `assetProtocol.enable` / `assetProtocol.scope`: keep it disabled or tightly scoped.
  - `app.withGlobalTauri`: default `false`; keep it.
  - `build.removeUnusedCommands`: strips commands that no capability allows (2.4+, [size docs](https://v2.tauri.app/concept/size/)).

**Isolation pattern** ([docs](https://v2.tauri.app/concept/inter-process-communication/isolation/)):
- Configure with `app.security.pattern = { "use": "isolation", "options": { "dir": "../dist-isolation" } }`.
- Every IPC message passes through a sandboxed iframe hook, `window.__TAURI_ISOLATION_HOOK__`, which can validate or modify it. The message is then AES-GCM encrypted with a key generated per launch, and the core decrypts it.
- Tauri "highly recommends" isolation whenever it can be used, especially against supply-chain-compromised frontend dependencies.
- **Windows limitation:** ES modules do not load in the sandboxed iframe on Windows. Tauri works around this with a build-time script-inlining step, so plain `<script src>` still works, but the isolation app must not depend on ES-module loading.
- Keys are generated with SubtleCrypto at every launch. Headless machines may need an entropy source (e.g. `haveged`).
- History: the Radically Open Security v2 audit report is published in the repo ([audit PDF](https://github.com/tauri-apps/tauri/blob/dev/audits/Radically_Open_Security-v2-report.pdf)). The claim that finding TAU2-040 was a key disclosure from the isolation iframe, fixed before 2.0, is **(unverified)**: the 2.0 blog does not mention it, and the PDF text could not be extracted for this check. Isolation is defense-in-depth. It does not replace Rust-side validation.

**Audit.** Radically Open Security audited Tauri 2 during the beta and RC phases, funded by NLnet via NGI. Per the 2.0 blog, the audit led to a rewrite of how the dev server is exposed for mobile development ([Tauri 2.0](https://v2.tauri.app/blog/tauri-20/)). The exact audit dates (Nov 2023–Aug 2024) and the claimed rewrites of iframe API exposure and scope validation are **(unverified)**.

### 4. Tauri IPC: commands, channels, events, raw payloads

From [Calling Rust](https://v2.tauri.app/develop/calling-rust/) and [Calling the frontend](https://v2.tauri.app/develop/calling-frontend/):

- **Commands.** Declare with `#[tauri::command]` and register with `tauri::generate_handler![…]` in `Builder::invoke_handler`.
  - `async fn` commands run off the main thread. They cannot take borrowed arguments (`&str`, `State<'_, T>`) unless the return type is `Result`; otherwise use owned types.
  - Errors must be `serde::Serialize`. Use `thiserror` and a `{kind, message}` shape.
  - Inject state with `tauri::State<T>` after `Builder::manage()`.
  - Commands defined in `lib.rs` cannot be `pub`.
- **Raw payloads.** `tauri::ipc::Request` exposes headers and an `InvokeBody::Raw` body. `tauri::ipc::Response::new(bytes)` returns an ArrayBuffer without JSON. The 2.0 IPC rewrite moved transport to custom protocols ([2.0 blog](https://v2.tauri.app/blog/tauri-20/)). This suits binary-encoded evidence-graph pages (e.g. MessagePack or FlatBuffers).
- **Channels.** `tauri::ipc::Channel<T>` is "designed to be fast and deliver ordered data". Tauri uses it internally for download progress. This is the right transport for scan progress, streaming findings and graph deltas.
- **Events.** `emit`, `emit_to` and `listen` carry JSON-string payloads only, are "not suitable for bigger messages", and are not type-safe. Use them only for broadcast notifications such as "quarantine expired". Unlisten handlers to avoid leaks.
- **Typed bindings.** `tauri-specta` 2.0.0-rc.25 generates TypeScript for commands and events, but it is still RC. `ts-rs` 12.0.1 is a stable alternative for DTO types only. Recommendation: use ts-rs or specta-generated DTOs plus a thin hand-written `invoke` wrapper, and pin the version.

### 5. Plugin system

From [Plugin development](https://v2.tauri.app/develop/plugins/):
- A plugin is a `tauri-plugin-<name>` crate with optional `guest-js/` bindings and optional Android (Kotlin) and iOS (Swift) native code.
- It is built with `tauri::plugin::Builder::new(name)`. Hooks: `setup`, `on_navigation` (return false to cancel navigation), `on_webview_ready`, `on_event`, `on_drop`.
- Commands are namespaced as `plugin:<name>|<cmd>`. Their permissions (`allow-x` / `deny-x`) are generated from `const COMMANDS` in `build.rs`.
- Lumen use: package each platform adapter's UI-facing surface as an internal plugin (`tauri-plugin-lumen-core`). That keeps the permission model granular, and `on_navigation` can block any navigation away from bundled assets.

### 6. Sidecars, background work and helper processes in Tauri

- **Sidecars** ([docs](https://v2.tauri.app/develop/sidecar/)) are declared in `bundle.externalBin`. Binaries are suffixed with the target triple (`rustc --print host-tuple`).
  - Run them from Rust with `app.shell().sidecar("name")`, or from JS with `Command.sidecar()`.
  - The JS route needs `shell:allow-execute` or `shell:allow-spawn` with `sidecar: true` and argument validators (static strings or regex). `args: true` allows **any** arguments.
- **Lifecycle gap.** Tauri does not reliably kill sidecars on every exit path. On macOS, Cmd+Q maps to `RunEvent::Exit`, not `ExitRequested` **(unverified against current docs; the sidecar page says nothing about cleanup)**. Apps must store the `CommandChild` and kill it in `RunEvent::Exit`. Upstream PR #14443 (adds a `cleanup_before_exit` plugin hook and makes the CLI kill the whole process tree) was **merged 2026-09-18 and reverted 2026-09-25** in PR #16134. The cause was a deadlock when a plugin calls `cleanup_before_exit` from its own callback, which is what `tauri-plugin-single-instance` does on a second launch. The analysis and a proposed v3 implementation are in #16090, and nothing has re-landed yet ([PR #14443](https://github.com/tauri-apps/tauri/pull/14443), [revert #16134](https://github.com/tauri-apps/tauri/pull/16134), [#16090](https://github.com/tauri-apps/tauri/pull/16090)).
- **Sidecars are the wrong primitive for a daemon or privileged helper.** They are children of the UI process, have the UI's privileges, and die (or leak) with it.
- **Shipping a real agent or daemon with Tauri:**
  - macOS: place the helper binary and its launchd plist inside the bundle with `bundle.macOS.files`. Its keys are paths relative to `<App>.app/Contents`, so `"Library/LaunchAgents/com.lumen.agent.plist"` lands at `Contents/Library/LaunchAgents/…`, and the same pattern works for `Library/LaunchDaemons/com.lumen.helper.plist` ([macOS bundle](https://v2.tauri.app/distribute/macos-application-bundle/)). Register them at runtime through `SMAppService`, called from Rust via `objc2-service-management` 0.3.2. Its docs.rs page confirms `agentServiceWithPlistName`, `daemonServiceWithPlistName`, `registerAndReturnError`, `unregisterAndReturnError`, `status` and `openSystemSettingsLoginItems` ([docs.rs](https://docs.rs/objc2-service-management/0.3.2/objc2_service_management/struct.SMAppService.html)). A small Swift shim is still an option. Add entitlements via `bundle.macOS.entitlements`.
  - Windows: use the NSIS hooks `NSIS_HOOK_POSTINSTALL` and `NSIS_HOOK_PREUNINSTALL` to register or unregister a service or scheduled task (`NSIS_HOOK_PREINSTALL` and `NSIS_HOOK_POSTUNINSTALL` also exist). `installMode: perMachine` is required for anything under Program Files or SCM ([Windows installer](https://v2.tauri.app/distribute/windows-installer/)).
- **Autostart:** `tauri-plugin-autostart` 2.7.0 offers `MacosLauncher::LaunchAgent` or `MacosLauncher::AppleScript` on macOS. It is built on the `auto-launch` 0.6 crate, which writes a legacy per-user LaunchAgent plist rather than using `SMAppService` ([docs](https://v2.tauri.app/plugin/autostart/), [docs.rs](https://docs.rs/tauri-plugin-autostart/2.7.0/tauri_plugin_autostart/enum.MacosLauncher.html)). It registers the *UI app* at login; it does not register a separate helper. **Single instance:** `tauri-plugin-single-instance` must be registered first and has no JS API ([docs](https://v2.tauri.app/plugin/single-instance/)).

### 7. Updater, signing, notarization, installers

**Updater** ([docs](https://v2.tauri.app/plugin/updater/)):
- Signature verification is mandatory and uses minisign-style keys from `tauri signer generate`. Set `pubkey` inline in `tauri.conf.json`.
- `endpoints` support `{{current_version}}`, `{{target}}` and `{{arch}}`. Use a static JSON manifest or a dynamic server that returns 204 when there is no update.
- Enable `bundle.createUpdaterArtifacts: true`. Artifacts: macOS `.app.tar.gz` + `.sig`; Windows NSIS/MSI + `.sig`.
- Windows `installMode` is `passive` (default), `basicUi` or `quiet`. Quiet needs admin. **The app exits on Windows during install.**
- Permissions: `updater:default` or granular `allow-check`, `allow-download`, `allow-install`.
- Lumen must also update any helper, agent or daemon atomically with the app. With `SMAppService`, the plists live inside the signed bundle, so replacing the bundle updates them. The helper must handle version skew (see §8).

**macOS signing** ([docs](https://v2.tauri.app/distribute/sign/macos/)):
- Use a "Developer ID Application" certificate for distribution outside the Mac App Store.
- Signing env vars: `APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY`.
- Notarization uses either an App Store Connect API key (`APPLE_API_KEY`, `APPLE_API_ISSUER`, `APPLE_API_KEY_PATH`) or an Apple ID (`APPLE_ID`, `APPLE_PASSWORD`, `APPLE_TEAM_ID`).
- Ad-hoc signing (`-`) does not avoid Gatekeeper prompts.
- Hardened Runtime is **required for notarization** ("To upload a macOS app to be notarized, you must enable the Hardened Runtime capability"). It disallows JIT and DYLD injection unless an entitlement opts back in. Request only what is needed. Every nested executable (agent, helper) must also be signed with the Hardened Runtime ([Hardened Runtime](https://developer.apple.com/documentation/security/hardened-runtime)).

**Windows signing** ([docs](https://v2.tauri.app/distribute/sign/windows/)):
- Configure `bundle.windows.certificateThumbprint`, `digestAlgorithm`, `timestampUrl`, or a custom `signCommand`.
- Supported `signCommand` routes include **Azure Artifact Signing** (formerly Trusted Signing) via `artifact-signing-cli`, and Azure Key Vault via `relic`.
- **EV certificates no longer get SmartScreen preference (since 2024).** Reputation builds the same way for OV and EV certificates.

**Windows installers** ([docs](https://v2.tauri.app/distribute/windows-installer/)):
- NSIS (cross-compilable) or MSI (WiX v3).
- WebView2 options: `downloadBootstrapper` (default, +0 MB), `embedBootstrapper` (~1.8 MB), `offlineInstaller` (~127 MB), `fixedVersion` (~180 MB).
- NSIS `installMode`: `currentUser` (default, `%LOCALAPPDATA%`, no admin), `perMachine` (Program Files, admin), `both` (user chooses, admin). The value is `currentUser`, not `perUser`.
- MSI can only be built on Windows, because WiX v3 is Windows-only.

### 8. Process architecture: UI vs agent vs privileged helper

**macOS**
- `SMAppService` (macOS 13+) covers login items, agents and daemons ([docs](https://developer.apple.com/documentation/servicemanagement/smappservice)).
  - `.agent(plistName:)`: the plist lives in `Contents/Library/LaunchAgents`. `register()` bootstraps it immediately and again at each login, and must be called once per user.
  - `.daemon(plistName:)`: the plist lives in `Contents/Library/LaunchDaemons`. It is **not bootstrapped until an admin approves it** in System Settings → Login Items.
  - Status values: `notRegistered`, `enabled`, **`requiresApproval`**, `notFound`. `SMAppService.openSystemSettingsLoginItems()` deep-links to the approval pane ([register()](https://developer.apple.com/documentation/servicemanagement/smappservice/register()), [Status](https://developer.apple.com/documentation/servicemanagement/smappservice/status-swift.enum)).
  - Plists inside the signed bundle "neither the system nor a third party can modify without breaking the code signature", and users can see which app owns the service ([Apple sample](https://developer.apple.com/documentation/servicemanagement/updating-your-app-package-installer-to-use-the-new-service-management-api)).
- `SMJobBless` is **deprecated since macOS 13** ([docs](https://developer.apple.com/documentation/servicemanagement/smjobbless(_:_:_:_:))). New code should not use it.
- **XPC:**
  - `XPCSession` and `XPCListener` are the modern Swift APIs (macOS 14+) ([Creating XPC services](https://developer.apple.com/documentation/xpc/creating-xpc-services)).
  - Client authentication: `xpc_connection_set_peer_code_signing_requirement` (C, macOS 12+) or `XPCPeerRequirement` (macOS 26+, e.g. `.isFromSameTeam(andMatchesSigningIdentifier:)`). `XPCListener.init(service:targetQueue:options:requirement:incomingSessionHandler:)` (macOS 26+) drops requests that fail the requirement ([XPCPeerRequirement](https://developer.apple.com/documentation/xpc/xpcpeerrequirement)).
  - This is the macOS mechanism for "only Lumen.app signed by our Team ID may ask the helper to move files".
- **App Sandbox** is required for the Mac App Store. Sandboxed apps cannot use Authorization Services, terminate other apps, or send Apple Events arbitrarily. Their file access is limited to the container plus user-selected files, and embedded tools inherit the sandbox ([Protecting user data](https://developer.apple.com/documentation/security/protecting-user-data-with-app-sandbox), [App Sandbox](https://developer.apple.com/documentation/security/app-sandbox)). A storage scanner whose value comes from looking across `~/Library`, other apps' caches and launch agents is fundamentally at odds with the sandbox. Default to Developer ID distribution.
- **Other apps' containers (macOS 14+), an omission in the first draft.** Since macOS 14, opening a file in *another developer's* app sandbox container (`~/Library/Containers/<other-app>`) makes the system ask the user for consent. The prompt text comes from `NSAppDataUsageDescription`; without that key a default message appears ([NSAppDataUsageDescription](https://developer.apple.com/documentation/bundleresources/information-property-list/nsappdatausagedescription)). Developer reports say the grant is per process lifetime, so the prompt can come back on every launch, and that macOS 15 extends similar protection to Group Containers **(third-party/forum reports; unverified)**. Implications for Lumen:
  - Set `NSAppDataUsageDescription` in the UI app.
  - Make the scanner handle denial: the result is "unknown", never "empty, safe to delete".
  - Do not have a headless agent trigger these prompts. Whether Full Disk Access suppresses them is **(unverified)**.
- **App Management (macOS 13+), an omission in the first draft.** A TCC "App Management" protection (`kTCCServiceSystemPolicyAppBundles`) blocks unauthorized modification or removal of app bundles. Third-party analyses report that Full Disk Access implies it **(unverified against Apple primary docs)**. Removing apps or app remnants inside bundles therefore needs explicit UX, and cleanup must never rely on silently modifying `.app` bundles ([Lapcat analysis](https://lapcatsoftware.com/articles/AppManagement.html)).
- **Cloud placeholders (both OSes), an omission in the first draft.** These are critical for a scanner's safety and correctness:
  - Windows: `FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS` and `FILE_ATTRIBUTE_RECALL_ON_OPEN` mark OneDrive and other cloud-files placeholders. Reading or opening them fetches content from the remote store ([File attribute constants](https://learn.microsoft.com/en-us/windows/win32/fileio/file-attribute-constants)).
  - macOS: iCloud and File Provider items expose `ubiquitousItemDownloadingStatusKey` ([docs](https://developer.apple.com/documentation/foundation/urlresourcekey/ubiquitousitemdownloadingstatuskey)).
  - The core must classify placeholders from metadata only, never hash or open them. It must report allocated (on-disk) size rather than logical size. Deleting a synced item can delete the cloud copy on every device, so the policy engine should treat cloud-synced trees as KEEP/REVIEW by default.
- Endpoint Security (`com.apple.developer.endpoint-security.client`) needs an Apple-granted entitlement ([docs](https://developer.apple.com/documentation/bundleresources/entitlements/com.apple.developer.endpoint-security.client)). Lumen does not need it for scanning and should avoid it unless real-time file-event attribution becomes a requirement.

**Windows**
- **Services:**
  - Rust options: `windows-service` 0.8.1, or Microsoft's `windows-services` 0.100.0 (2026-09-03, part of [windows-rs](https://github.com/microsoft/windows-rs)).
  - Prefer **LocalService** where possible: it has minimal local privileges and anonymous network credentials ([LocalService](https://learn.microsoft.com/en-us/windows/win32/services/localservice-account)). But deleting or moving files in system or other users' locations needs admin or LocalSystem-level rights, which argues for an *on-demand elevated* helper over a persistent LocalSystem service.
- **Named pipes** as the IPC transport:
  - The default DACL grants full control to LocalSystem, Administrators and the creator owner, and **read to Everyone and anonymous** ([Named pipe security](https://learn.microsoft.com/en-us/windows/win32/ipc/named-pipe-security-and-access-rights)).
  - When granting client access, use the individual rights (`FILE_READ_DATA`, `FILE_WRITE_DATA`, …), not `FILE_GENERIC_WRITE`. `FILE_APPEND_DATA` is the same bit as `FILE_CREATE_PIPE_INSTANCE`, so `FILE_GENERIC_WRITE` lets a client create pipe instances. Microsoft also recommends putting the logon SID on the DACL to block other terminal-services sessions (same source).
  - Always pass an explicit `SECURITY_ATTRIBUTES` that restricts access to the interactive user's SID or logon SID. Set `PIPE_REJECT_REMOTE_CLIENTS` and use `FILE_FLAG_FIRST_PIPE_INSTANCE` so a squatter cannot pre-create the pipe ([CreateNamedPipe](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-createnamedpipea)).
  - Identify the client with `GetNamedPipeClientProcessId` ([docs](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-getnamedpipeclientprocessid)), then verify that process image's Authenticode signer before honoring requests. PID reuse races mean the request also needs a nonce or handshake.
  - Rust crate: `interprocess` 2.4.4 (named pipes and Unix domain sockets) or `tokio::net::windows::named_pipe` (`tokio` 1.53.2).
- **Per-user agent:** a Task Scheduler logon task or an HKCU `Run` entry (which the autostart plugin uses) for unprivileged scheduled scans.

### 9. Bundle size and memory footprint

- Official: "a minimal Tauri app can be less than 600KB" ([Tauri start](https://v2.tauri.app/start/)). Size tuning in the release profile: `lto = true`, `codegen-units = 1`, `opt-level = "s"`, `panic = "abort"`, `strip = true`, plus `removeUnusedCommands` ([App size](https://v2.tauri.app/concept/size/)). Note that `panic = "abort"` interacts with UniFFI's panic-to-error mapping; see Risks.
- Independent 2026 comparisons (third-party, not primary; methodology varies):
  - Tauri apps are ~2–12 MB vs ~85–180 MB for Electron.
  - Idle RAM is ~30–85 MB vs ~170–450 MB for Electron.
  - Sources: [rustify.rs](https://rustify.rs/articles/rust-tauri-vs-electron-2026), [pkgpulse](https://www.pkgpulse.com/blog/best-desktop-app-frameworks-2026).
  - Treat these as order-of-magnitude only **(unverified)**. Webview helper processes (WebView2 / WebKit content processes) are often omitted from "app RAM" figures.

### 10. Mobile support status in Tauri

- iOS and Android have been supported since 2.0 (Swift and Kotlin plugin layers) ([Tauri 2.0](https://v2.tauri.app/blog/tauri-20/)), and 2.12 keeps Android tooling current ([2.12](https://v2.tauri.app/blog/tauri-2.12/)).
- Multi-webview is still behind the `unstable` feature ([2.0 blog](https://v2.tauri.app/blog/tauri-20/); current status **(unverified)**).
- For Lumen, mobile storage tools need deep native integration and background-execution APIs. A shared UniFFI core with native or Expo shells is likely a better mobile fit than Tauri mobile. That decision belongs to the mobile research doc.

### 11. Known Tauri limitations relevant to Lumen

1. Two rendering engines. WKWebView is pinned to the OS version: an older macOS means older WebKit.
2. No built-in daemon or helper model, and sidecar cleanup on exit is still unsolved upstream (§6).
3. Isolation pattern: no ES modules in the sandboxed iframe on Windows. Tauri inlines scripts at build time to compensate.
4. Capabilities cannot tell an iframe from its window on Linux and Android.
5. Event payloads are JSON strings only.
6. Typed bindings (`tauri-specta`) are still RC.
7. Next.js needs `output: 'export'` and `images.unoptimized`. There is no SSR inside Tauri ([Next.js guide](https://v2.tauri.app/start/frontend/nextjs/)). If the web dashboard is Next.js (`next` 16.3.8), shared UI must live in a framework-agnostic React package (`react` 19.3.0).
8. The docs are inconsistent: the webview-versions page still says Windows 7 is supported, but 2.12 removed it.

### 12. Alternatives

**A. Native SwiftUI (macOS) + Rust core via FFI**
- **UniFFI 0.32.2** (crates.io, 2026-09-23). It generates a C header, a modulemap and Swift sources, packaged as an XCFramework.
  - Rust `Result<T, E>` becomes Swift `throws`. Panics become a private `Error` type that is fatal in non-throwing contexts.
  - Most generated code is `Sendable`, but async has gaps (issue #2448) ([Swift overview](https://mozilla.github.io/uniffi-rs/latest/swift/overview.html)).
  - Recent versions add zero-copy `&[u8]` arguments (0.32.0; not yet in async functions), and `&mut [u8]` (sync only) is unreleased (per the [changelog](https://raw.githubusercontent.com/mozilla/uniffi-rs/main/CHANGELOG.md)). Methods on records and enums arrived in **0.31.0** (2026-01-14), not 0.30. Note that 0.32.2 is on crates.io (2026-09-23), but the `main` changelog has no 0.32.2 entry; its latest entry is 0.32.1.
- **swift-bridge 0.1.59** (2026-01): finer-grained, zero-copy-oriented, but pre-1.0 and Swift-only. **cbindgen 0.29.4**: a C header only; you hand-write a safe Swift wrapper.
- Pros: best macOS fidelity (menus, accessibility, Settings, `SMAppService`/XPC natively in Swift). Cons: a separate UI codebase per OS, and no reuse with the React web dashboard.

**B. WinUI 3 / Windows App SDK + Rust**
- Windows App SDK **2.0** went GA 2026-04-29. The latest stable patch is **2.5.1** (2026-09-16), and 1.8 left servicing on 2026-09-24 ([release channels](https://learn.microsoft.com/en-us/windows/apps/windows-app-sdk/release-channels)).
- Bridging C# to Rust:
  - `uniffi-bindgen-cs` v0.11.0, which targets UniFFI 0.31.0 and .NET 8+ ([repo](https://github.com/NordSecurity/uniffi-bindgen-cs)).
  - `csbindgen` 1.9.8 (generates C# P/Invoke).
  - `interoptopus` 0.16.5.
- Pure Rust:
  - `windows` 0.62.2 / `windows-sys` 0.61.2 umbrella crates.
  - Newer focused crates at 0.100.0 (2026-09-03): `windows-core`, `windows-registry`, `windows-services`, `windows-webview`.
  - The experimental `windows-reactor` 0.100.0, "Declarative Windows UI library for Rust… backed by WinUI 3" ([windows-rs](https://github.com/microsoft/windows-rs)), is too new for production **(maturity unverified)**.
- Pros: native Fluent UI. Cons: a second full UI stack, a C# toolchain, the UniFFI version lag, and packaged vs unpackaged deployment complexity.

**C. Electron (44.5.1 latest on npm, 2026-09-30)**
- Mature, with good security defaults: `contextIsolation` since 12, sandbox since 20, Node integration off since 5. A 20-item checklist covers fuses and IPC sender validation ([Electron security](https://www.electronjs.org/docs/latest/tutorial/security)).
- **Why not by default for Lumen:**
  - It bundles Chromium plus Node, which means a large download and RAM footprint. That hurts credibility for a product whose job is reclaiming space.
  - The privileged surface is a Node main process, and the Rust core would need N-API bindings (napi-rs) or a sidecar.
  - Its update and security patch cadence is entirely Lumen's responsibility.
  - There is no advantage for a Rust-core architecture.

**D. Rust-native UIs**
- **Slint 1.18.1**:
  - Declarative and compiled, with a native-feeling look.
  - Licensed GPLv3, a royalty-free license for desktop/mobile proprietary apps, or paid tiers ([pricing](https://slint.dev/pricing)).
  - Good for small native UIs, but it has no web-dashboard reuse.
- **egui / eframe 0.36.2**:
  - Immediate mode, with AccessKit accessibility on Windows and macOS.
  - Explicitly "not designed to look native". Large scroll areas can be slow ([egui](https://github.com/emilk/egui)).
  - Good for internal or debug tools such as an evidence-graph inspector, not the consumer UI.
- **Dioxus 0.7.10** (0.8.0-alpha.1):
  - React-like Rust UI. Desktop uses a webview (wry), with the experimental WGPU "Blitz" native renderer ([Dioxus 0.7](https://dioxuslabs.com/learn/0.7/)).
  - All-Rust UI code, but it cannot share React components with the web dashboard, and its ecosystem is less mature.

### 13. Exposing the Rust core safely (Swift / Kotlin / C# / TypeScript)

**UniFFI essentials (0.32.x)**
- **Setup:** `uniffi::setup_scaffolding!()` with proc-macros (library mode). Export with `#[uniffi::export]`; derive `uniffi::Record`, `Enum`, `Object` and `Error` ([proc-macros](https://mozilla.github.io/uniffi-rs/latest/proc_macro/index.html)). Gate it as `#[cfg_attr(feature = "uniffi", uniffi::export)]` so core crates stay FFI-agnostic.
- **Async:**
  - Rust `async fn` becomes Swift `async` and Kotlin `suspend`, and the **foreign side drives the future** ([futures](https://mozilla.github.io/uniffi-rs/latest/futures.html)).
  - For Tokio-dependent code, use `#[uniffi::export(async_runtime = "tokio")]`. It wraps the scaffolding future in `async_compat::Compat`, and since 0.32.0 it can also be applied to trait exports (per the changelog, #2899; the futures page does not document it). Alternatively, keep a core-owned Tokio runtime and expose futures that only await channels.
  - **There is no cancellation.** Expose explicit `cancel()` methods or cancellation tokens.
- **Errors:**
  - `#[derive(uniffi::Error)]` enums map to typed exceptions. `#[uniffi(flat_error)]` serializes the error via `Display` only ([errors](https://mozilla.github.io/uniffi-rs/latest/proc_macro/errors.html)).
  - Foreign trait or callback errors need `From<uniffi::UnexpectedUniFFICallbackError>`; otherwise unexpected errors panic ([foreign traits](https://mozilla.github.io/uniffi-rs/latest/foreign_traits.html)).
- **Foreign traits** (`#[uniffi::export(foreign)]` / `(rust, foreign)`) must be `Send + Sync + Debug`, all parameters are passed by value, and Rust↔foreign reference cycles leak. Use them for platform adapter ports implemented in Swift or Kotlin, such as a `PhotoLibraryPort` on iOS.
- **Stability:** "ready for production use… a long way from a 1.0"; advanced features can break between versions ([README](https://github.com/mozilla/uniffi-rs/blob/main/README.md)). Third-party generators lag: C# is at 0.31.0, React Native at 0.31.0-6, and Kotlin Multiplatform (Gobley) at `gobley-uniffi-bindgen` 0.3.7.

**Facade pattern for Lumen**
- `lumen-core` crates (domain, policy, evidence graph) have **no** FFI or Tauri dependencies.
- `lumen-app` is the application-service layer, the hexagonal "driving ports": `ScanService`, `PlanService`, `QuarantineService`, `QueryService`.
- Adapters:
  - `lumen-tauri`: commands, channels, capability permissions. TypeScript types come from ts-rs or specta.
  - `lumen-ffi`: UniFFI DTOs and objects for Swift, Kotlin and C#.
  - `lumen-ipc`: a versioned, serde-based protocol for agent and helper IPC.
- Rules:
  - All three expose **DTOs, not domain types**.
  - They are coarse-grained and versioned. `api_version` is negotiated at connect time.
  - Destructive operations take **opaque plan IDs**, not paths.
- Emerging alternatives to watch, not adopt yet:
  - `boltffi` 0.31.0 claims much higher FFI throughput than UniFFI **(claims unverified)**.
  - `diplomat` 0.16.1 (ICU4X's multi-language FFI).
  - `flutter_rust_bridge` 2.13.0 if Flutter were chosen.

## Implications for Lumen

### Recommended architecture: "Tauri shell + in-host core + split-privilege helpers"

```
┌──────────────────────── Lumen.app / Lumen.exe (user privileges) ────────────────────────┐
│  WebView (untrusted): React+TS UI (shared design system with web dashboard)             │
│     │  invoke / Channel  (CSP, isolation pattern, per-window capability, no fs/shell)   │
│  Tauri Rust host (trusted): lumen-tauri adapter → lumen-app services → lumen-core       │
│     • scanner adapters (read-only)   • evidence graph   • policy engine (deterministic)│
│     • quarantine executor for USER-OWNED paths (move→verify→journal→rollback)          │
│     • Jev adapter (evidence only)    • SQLite state (rusqlite 0.40.x)                  │
└────────────┬────────────────────────────────────────────────┬──────────────────────────┘
             │ lumen-ipc (UDS / named pipe, ACL'd, versioned)  │ XPC (peer req) / elevated pipe
┌────────────▼──────────────┐                      ┌───────────▼──────────────────────────┐
│ lumen-agent (per-user,     │                      │ lumen-helper (root/admin, OPTIONAL)  │
│ headless, same core)       │                      │ tiny verb set: stat, quarantine-move,│
│ SMAppService.agent /       │                      │ restore, for system-scope paths that │
│ Task Scheduler logon task  │                      │ policy already approved by plan ID   │
│ scheduled scans, expiry    │                      │ SMAppService.daemon / UAC on demand  │
└────────────────────────────┘                      └──────────────────────────────────────┘
```

1. **Shell: Tauri 2.12.x (pin `=2.12.1`, Rust ≥ 1.90).** Choose Tauri over native UIs because of one React UI codebase shared with the web dashboard, a small footprint, a security model that matches Lumen's "webview is untrusted" stance, and a Rust host where the core runs without FFI.
   - *Rejected:* native SwiftUI + WinUI. It is the best fidelity, but it doubles UI cost, and the C# path is gated by the UniFFI lag.
   - *Rejected:* Electron. Footprint, an extra Node attack surface, and it buys nothing for a Rust core.
   - *Rejected:* Slint, egui and Dioxus for the main UI. No dashboard reuse, plus weaker data-viz and accessibility ecosystems. egui remains a good choice for an internal graph-inspector dev tool.
2. **The core runs in the Tauri host process, not a sidecar.** The webview is already out-of-process and untrusted. A sidecar would add lifecycle bugs (§6) without moving the trust boundary.
3. **Command API design is the main safety control:**
   - Read model: `get_scan_snapshot`, `query_graph(page)`. These return DTOs, with large pages sent via `ipc::Response`.
   - Proposal: `propose_cleanup_plan(selection_ids) -> PlanId`. The core resolves IDs to paths and re-runs policy. KEEP and REVIEW can never be executed silently.
   - Execution: `execute_plan(plan_id, user_confirmation_token)`. It streams progress over a `Channel`, verifies, journals, and returns a rollback handle. `rollback(plan_id)` undoes it.
   - Scanner adapters classify cloud placeholders (Windows `RECALL_ON_*` attributes, macOS ubiquitous/File Provider items) and TCC-denied locations from metadata only. They surface them as "not local" or "unknown", and policy can never put either in an executable plan (see §8).
   - **No command accepts a raw filesystem path for mutation.** Do not register `tauri-plugin-fs`, `tauri-plugin-shell` or the asset protocol for the main window.
   - Define every app command as its own permission via `AppManifest`. Enable `removeUnusedCommands`, `freezePrototype`, a strict CSP, the isolation pattern (with an inlined script for Windows), and `on_navigation` denial. Deny all webview permission requests via `on_permission_request`.
4. **Background agent (v1.x):** the same Rust binary in `--agent` mode, or a small separate binary, with no webview.
   - macOS: `SMAppService.agent`. Windows: a Task Scheduler logon task. Avoid the HKCU Run key for the agent so that it does not start the full UI.
   - It runs scheduled read-only scans and quarantine expiry or purge **after** a retention period.
   - Single-writer rule: the agent owns the state DB when it is running, and the UI becomes a client over `lumen-ipc`. Otherwise the UI owns the DB.
   - *Rejected:* making the agent mandatory in v1. That adds IPC, approval UX and update coupling before there is evidence users want background scans.
5. **Privileged helper (later, opt-in):**
   - Ship it only when Lumen needs system-scope paths: `/Library/Caches`, other users' files, `C:\Windows\Temp`, Program Files remnants.
   - Write it in Rust with a minimal dependency set. The verb set is `stat`, `quarantine_move`, `restore` and `verify_hash`. It accepts only a signed plan manifest produced by the core and re-checks paths against a hard-coded system deny-list (SIP-protected paths, `C:\Windows\System32`, …).
   - macOS: `SMAppService.daemon` (the user approves in System Settings; handle `requiresApproval`). Use XPC with `xpc_connection_set_peer_code_signing_requirement` set to the Lumen Team ID and bundle ID, and move to `XPCPeerRequirement` when the minimum OS is ≥ 26.
   - Windows: prefer **on-demand elevation**. `ShellExecuteEx` with the `runas` verb starts `lumen-helper.exe` for one batch, using a one-shot, DACL-restricted pipe with `PIPE_REJECT_REMOTE_CLIENTS`. This avoids a persistent LocalSystem service.
   - *Rejected:* `SMJobBless` (deprecated), a persistent LocalSystem service by default (standing attack surface), and running the whole app elevated.
6. **Bindings strategy:** create `lumen-ffi` (UniFFI 0.32.x) now, even though desktop does not need it. iOS and Android will use it, and it keeps a later native SwiftUI or WinUI shell possible without touching the core.
   - Keep the `lumen-app` service traits FFI-friendly: owned arguments, `Result<_, LumenError>`, explicit cancellation tokens.
   - Generate TypeScript DTOs from the same Rust types (ts-rs) for both Tauri and the web dashboard.
7. **Distribution:**
   - macOS: Developer ID + notarization via the App Store Connect API key in CI, with Hardened Runtime (which notarization requires) on the app and on every nested agent and helper binary, and no JIT entitlements. Set `minimumSystemVersion = 13.0`. Add `NSAppDataUsageDescription` for the macOS 14+ other-app-container prompt. The Mac App Store is out of scope (App Sandbox conflict).
   - Windows: NSIS `installMode: "currentUser"` (the default) for the base app; an elevated on-demand helper does not require a perMachine install, but a persistent SCM service would. Use the WebView2 `downloadBootstrapper` and offer an `offlineInstaller` variant for enterprises. Sign with Azure Artifact Signing.
   - Updater: Tauri updater with offline-stored minisign keys. Helper version-skew checks happen in the IPC handshake.
8. **Watch Tauri 3 and CEF.** The CEF runtime could remove WebKit/Chromium divergence at the cost of size. Re-evaluate when v3 reaches RC. Do not use `macos-private-api`, because v3 removes it.

## Risks and open questions

- **Upstream maturity:**
  - Tauri sidecar exit handling is unresolved (PR #14443 reverted).
  - `tauri-specta` is RC.
  - UniFFI is pre-1.0, and its async Swift 6 `Sendable` support is incomplete (issue #2448).
  - Third-party UniFFI generators lag (C# at 0.31).
  - Mitigation: pin exact versions, add contract tests at the FFI and IPC boundaries, and keep a thin adapter layer.
- **`panic = "abort"` vs FFI.** Tauri's size guide recommends `panic = "abort"`, but UniFFI converts panics into errors only when unwinding. Use `panic = "unwind"` for the `lumen-ffi` cdylib, and probably for the desktop host too, so one adapter panic does not kill the app in the middle of a quarantine. Rely on journaled two-phase moves for crash safety either way.
- **TCC / Full Disk Access attribution:** which binary (UI app, LaunchAgent, LaunchDaemon) must the user grant Full Disk Access to, and does an `SMAppService` agent inherit the app's grant? **(unverified; needs hands-on testing on macOS 15/26/27.)** This drives onboarding UX.
- **`SMAppService` from Rust:** `objc2-service-management` 0.3.2 exposes `register`/`unregister` (`registerAndReturnError`, `unregisterAndReturnError`), `status`, `openSystemSettingsLoginItems` and the agent and daemon constructors. This is confirmed on docs.rs. Runtime behavior is still untested. The fallback is a ~50-line Swift shim linked into the host.
- **XPC from Rust:** the modern Swift `XPCSession`/`XPCListener` APIs have no first-class Rust bindings (`xpc-connection` 0.2.3 is from 2020). Options: the libxpc C API via FFI, a Swift helper shim, or a UDS with `getpeereid` + `SecCode` signature checks. Pick one in an ADR.
- **Windows client verification:** PID-based Authenticode checks via `GetNamedPipeClientProcessId` are racy. Is a per-session random pipe name passed at elevation time sufficient? **(open)**
- **WebKit vs Chromium rendering differences** in data-dense graph views (canvas/WebGL performance on WKWebView). Benchmark early with a realistic evidence-graph size.
- **Memory and size numbers** in this doc are third-party. Measure Lumen's own idle and active RSS, including webview helper processes, on both OSes.
- **Single-writer coordination between UI and agent** (lock file vs agent-as-server) needs an ADR, including behavior when the agent is unapproved or disabled.
- **Windows 10 support horizon:** Tauri 2.12 still supports Windows 10, and Windows App SDK is backward compatible to 1809 but supported only on in-support Windows releases. Windows 10 itself left support on 2025-10-14. Microsoft says Edge and the **WebView2 Runtime will keep receiving updates on Windows 10 22H2 until at least October 2028, without requiring ESU** ([Edge supported OSes](https://learn.microsoft.com/en-us/deployedge/microsoft-edge-supported-operating-systems)). So supporting Windows 10 22H2 is viable for the webview layer through 2028, but the OS underneath is unpatched for non-ESU users. Lumen's own support matrix is undecided.
- **Cloud placeholders and macOS 14+ container prompts** (§8) need explicit scanner semantics: an "unknown" or "not local" state that the policy engine can never turn into a delete.
- **Mac App Store variant:** is a reduced, sandboxed "Lite" SKU worth it? Not recommended initially.

## Sources

- [crates.io API: tauri, tauri-build, wry, tao, uniffi, swift-bridge, cbindgen, windows, windows-sys, windows-core, windows-services, windows-reactor, windows-webview, slint, egui, eframe, dioxus, tauri plugins, tokio, interprocess, csbindgen, interoptopus, boltffi, diplomat, objc2-service-management, ts-rs, tauri-specta, rusqlite](https://crates.io/)
- [npm registry: @tauri-apps/cli, @tauri-apps/api, @tauri-apps/plugin-updater, @tauri-apps/plugin-shell, electron, react, next, expo, uniffi-bindgen-react-native](https://www.npmjs.com/)
- [Tauri: Announcing Tauri 2.12](https://v2.tauri.app/blog/tauri-2.12/)
- [Tauri: Tauri 2.0 Stable Release](https://v2.tauri.app/blog/tauri-20/)
- [Tauri blog index](https://v2.tauri.app/blog/)
- [Tauri: Security overview](https://v2.tauri.app/security/)
- [Tauri: Capabilities](https://v2.tauri.app/security/capabilities/)
- [Tauri: Permissions](https://v2.tauri.app/security/permissions/)
- [Tauri: Command Scopes](https://v2.tauri.app/security/scope/)
- [Tauri: Content Security Policy](https://v2.tauri.app/security/csp/)
- [Tauri: Isolation Pattern](https://v2.tauri.app/concept/inter-process-communication/isolation/)
- [Tauri: Architecture](https://v2.tauri.app/concept/architecture/)
- [Tauri: Calling Rust from the Frontend](https://v2.tauri.app/develop/calling-rust/)
- [Tauri: Calling the Frontend from Rust](https://v2.tauri.app/develop/calling-frontend/)
- [Tauri: Plugin Development](https://v2.tauri.app/develop/plugins/)
- [Tauri: Embedding External Binaries (sidecar)](https://v2.tauri.app/develop/sidecar/)
- [Tauri: Updater plugin](https://v2.tauri.app/plugin/updater/)
- [Tauri: Autostart plugin](https://v2.tauri.app/plugin/autostart/)
- [Tauri: Single Instance plugin](https://v2.tauri.app/plugin/single-instance/)
- [Tauri: macOS Code Signing](https://v2.tauri.app/distribute/sign/macos/)
- [Tauri: Windows Code Signing](https://v2.tauri.app/distribute/sign/windows/)
- [Tauri: Windows Installer](https://v2.tauri.app/distribute/windows-installer/)
- [Tauri: macOS Application Bundle](https://v2.tauri.app/distribute/macos-application-bundle/)
- [Tauri: App Size](https://v2.tauri.app/concept/size/)
- [Tauri: Webview Versions](https://v2.tauri.app/reference/webview-versions/)
- [Tauri: Configuration reference](https://v2.tauri.app/reference/config/)
- [Tauri: What is Tauri](https://v2.tauri.app/start/)
- [Tauri: Next.js frontend guide](https://v2.tauri.app/start/frontend/nextjs/)
- [GitHub: tauri-apps/tauri releases](https://github.com/tauri-apps/tauri/releases)
- [GitHub: tauri-runtime-cef v3.0.0-alpha.4 release](https://github.com/tauri-apps/tauri/releases/tag/tauri-runtime-cef-v3.0.0-alpha.4)
- [GitHub: tauri PR #14443 (cleanup_before_exit; merged then reverted)](https://github.com/tauri-apps/tauri/pull/14443)
- [Radically Open Security: Tauri 2.0 penetration test report](https://github.com/tauri-apps/tauri/blob/dev/audits/Radically_Open_Security-v2-report.pdf)
- [GitHub: specta-rs/tauri-specta](https://github.com/specta-rs/tauri-specta)
- [UniFFI CHANGELOG](https://raw.githubusercontent.com/mozilla/uniffi-rs/main/CHANGELOG.md)
- [UniFFI README (third-party bindings, stability)](https://github.com/mozilla/uniffi-rs/blob/main/README.md)
- [UniFFI user guide](https://mozilla.github.io/uniffi-rs/latest/)
- [UniFFI: Async/Future support](https://mozilla.github.io/uniffi-rs/latest/futures.html)
- [UniFFI: Procedural macros](https://mozilla.github.io/uniffi-rs/latest/proc_macro/index.html)
- [UniFFI: Errors (proc-macro)](https://mozilla.github.io/uniffi-rs/latest/proc_macro/errors.html)
- [UniFFI: Swift overview](https://mozilla.github.io/uniffi-rs/latest/swift/overview.html)
- [UniFFI: Foreign traits](https://mozilla.github.io/uniffi-rs/latest/foreign_traits.html)
- [GitHub: NordSecurity/uniffi-bindgen-cs](https://github.com/NordSecurity/uniffi-bindgen-cs)
- [Apple: SMAppService](https://developer.apple.com/documentation/servicemanagement/smappservice)
- [Apple: SMAppService.daemon(plistName:)](https://developer.apple.com/documentation/servicemanagement/smappservice/daemon(plistname:))
- [Apple: SMAppService.agent(plistName:)](https://developer.apple.com/documentation/servicemanagement/smappservice/agent(plistname:))
- [Apple: SMAppService.register()](https://developer.apple.com/documentation/servicemanagement/smappservice/register())
- [Apple: SMAppService.Status](https://developer.apple.com/documentation/servicemanagement/smappservice/status-swift.enum)
- [Apple: openSystemSettingsLoginItems()](https://developer.apple.com/documentation/servicemanagement/smappservice/opensystemsettingsloginitems())
- [Apple: SMJobBless (deprecated)](https://developer.apple.com/documentation/servicemanagement/smjobbless(_:_:_:_:))
- [Apple: Updating your app package installer to use the new Service Management API](https://developer.apple.com/documentation/servicemanagement/updating-your-app-package-installer-to-use-the-new-service-management-api)
- [Apple: XPC framework](https://developer.apple.com/documentation/xpc)
- [Apple: XPCSession](https://developer.apple.com/documentation/xpc/xpcsession)
- [Apple: XPCListener](https://developer.apple.com/documentation/xpc/xpclistener)
- [Apple: XPCPeerRequirement](https://developer.apple.com/documentation/xpc/xpcpeerrequirement)
- [Apple: xpc_connection_set_peer_code_signing_requirement](https://developer.apple.com/documentation/xpc/xpc_connection_set_peer_code_signing_requirement(_:_:))
- [Apple: Creating XPC services](https://developer.apple.com/documentation/xpc/creating-xpc-services)
- [Apple: Hardened Runtime](https://developer.apple.com/documentation/security/hardened-runtime)
- [Apple: App Sandbox](https://developer.apple.com/documentation/security/app-sandbox)
- [Apple: Protecting user data with App Sandbox](https://developer.apple.com/documentation/security/protecting-user-data-with-app-sandbox)
- [Apple: Accessing files from the macOS App Sandbox](https://developer.apple.com/documentation/security/accessing-files-from-the-macos-app-sandbox)
- [Apple: Endpoint Security client entitlement](https://developer.apple.com/documentation/bundleresources/entitlements/com.apple.developer.endpoint-security.client)
- [Microsoft Learn: Windows App SDK release channels](https://learn.microsoft.com/en-us/windows/apps/windows-app-sdk/release-channels)
- [Microsoft Learn: Named Pipe Security and Access Rights](https://learn.microsoft.com/en-us/windows/win32/ipc/named-pipe-security-and-access-rights)
- [Microsoft Learn: CreateNamedPipeA](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-createnamedpipea)
- [Microsoft Learn: GetNamedPipeClientProcessId](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-getnamedpipeclientprocessid)
- [Microsoft Learn: LocalService Account](https://learn.microsoft.com/en-us/windows/win32/services/localservice-account)
- [GitHub: microsoft/windows-rs](https://github.com/microsoft/windows-rs)
- [GitHub: microsoft/windows-rs releases](https://github.com/microsoft/windows-rs/releases)
- [Electron: Security checklist](https://www.electronjs.org/docs/latest/tutorial/security)
- [Slint: Pricing / licensing](https://slint.dev/pricing)
- [Dioxus 0.7 docs](https://dioxuslabs.com/learn/0.7/)
- [GitHub: emilk/egui](https://github.com/emilk/egui)
- [rustify.rs: Tauri vs Electron 2026 (third-party benchmark)](https://rustify.rs/articles/rust-tauri-vs-electron-2026)
- [PkgPulse: Tauri vs Electron 2026 (third-party benchmark)](https://www.pkgpulse.com/blog/best-desktop-app-frameworks-2026)
- [GitHub: tauri PR #16134 (revert of #14443)](https://github.com/tauri-apps/tauri/pull/16134)
- [GitHub: tauri PR #16090 (cleanup_before_exit deadlock analysis)](https://github.com/tauri-apps/tauri/pull/16090)
- [GitHub: uniffi-rs issue #2448 (Swift 6 Sendable)](https://github.com/mozilla/uniffi-rs/issues/2448)
- [docs.rs: objc2-service-management 0.3.2 SMAppService](https://docs.rs/objc2-service-management/0.3.2/objc2_service_management/struct.SMAppService.html)
- [docs.rs: tauri-plugin-autostart MacosLauncher](https://docs.rs/tauri-plugin-autostart/2.7.0/tauri_plugin_autostart/enum.MacosLauncher.html)
- [Apple: NSAppDataUsageDescription (macOS 14+)](https://developer.apple.com/documentation/bundleresources/information-property-list/nsappdatausagedescription)
- [Apple: ubiquitousItemDownloadingStatusKey](https://developer.apple.com/documentation/foundation/urlresourcekey/ubiquitousitemdownloadingstatuskey)
- [Apple Developer Forums: Installer.app asks permission before writing App Sandbox Data Container since macOS 14](https://developer.apple.com/forums/thread/739602)
- [Lapcat Software: How macOS Ventura App Management works (third-party)](https://lapcatsoftware.com/articles/AppManagement.html)
- [Microsoft Learn: File Attribute Constants (cloud placeholder attributes)](https://learn.microsoft.com/en-us/windows/win32/fileio/file-attribute-constants)
- [Microsoft Learn: Microsoft Edge supported operating systems (WebView2 on Windows 10 through Oct 2028)](https://learn.microsoft.com/en-us/deployedge/microsoft-edge-supported-operating-systems)

## Verification log

Fact-check performed 2026-10-05 against primary sources: the crates.io and npm registry APIs, Tauri docs, Apple developer documentation JSON, Microsoft Learn, and GitHub.

| # | Claim | Verdict | Source |
| --- | --- | --- | --- |
| 1 | `tauri` 2.12.1 stable, 3.0.0-alpha.4 pre-release; `tauri-build` 2.7.1; `@tauri-apps/cli`/`api` 2.12.1 (2026-09-30); `tauri-runtime-cef` 3.0.0-alpha.5 only | confirmed | crates.io API, npm registry |
| 2 | Plugin/crate versions: wry 0.57.0, tao 0.37.1, updater 2.13.1, shell 2.4.0, single-instance 2.5.2, autostart 2.7.0, tauri-specta 2.0.0-rc.25, uniffi 0.32.2 (2026-09-23), tokio 1.53.2, interprocess 2.4.4, windows-services 0.100.0, rusqlite 0.40.2, ts-rs 12.0.1, slint 1.18.1, egui 0.36.2, dioxus 0.7.10/0.8.0-alpha.1, xpc-connection 0.2.3 (2020), electron 44.5.1, next 16.3.8, react 19.3.0, uniffi-bindgen-react-native 0.31.0-6 | confirmed | crates.io API, npm registry |
| 3 | Tauri 2.12 (2026-09-26) drops Windows 7, MSRV 1.90 with a "stable − 3" policy, adds `on_permission_request`, targetSdk 37, Gradle 9/Kotlin 2; no Tauri 3/CEF mention | confirmed | v2.tauri.app/blog/tauri-2.12 |
| 4 | Webview-versions page still says WebView2 supports Windows 7 | confirmed (docs inconsistency) | v2.tauri.app/reference/webview-versions |
| 5 | PR #14443 merged 2026-09-18, reverted 2026-09-25 for a deadlock | confirmed; added revert PR #16134, #16090 and the fact that nothing has re-landed | GitHub PRs #14443, #16134, #16090 |
| 6 | `SMAppService` agent/daemon macOS 13+; daemon not bootstrapped until admin approval; Status cases; `SMJobBless` deprecated 13.0 ("Please use SMAppService instead") | confirmed | Apple docs (daemon, agent, register(), Status, SMJobBless) |
| 7 | `xpc_connection_set_peer_code_signing_requirement` macOS 12+; `XPCPeerRequirement` and `XPCListener(…requirement:…)` macOS 26+; `XPCSession`/`XPCListener` macOS 14+ | confirmed | Apple XPC docs |
| 8 | Named-pipe default security descriptor grants read to Everyone/anonymous | confirmed; corrected wording (NULL security attributes vs NULL DACL) and added the `FILE_GENERIC_WRITE` → `FILE_CREATE_PIPE_INSTANCE` pitfall | Microsoft Learn: Named Pipe Security |
| 9 | Windows App SDK 2.0 GA 2026-04-29, 2.5.1 latest stable (2026-09-16), 1.8 servicing ends 2026-09-24 | confirmed | Microsoft Learn: release channels |
| 10 | Tauri Windows signing: EV loses SmartScreen preference since 2024; Azure Artifact Signing via `artifact-signing-cli`; relic for Key Vault | confirmed | v2.tauri.app/distribute/sign/windows |
| 11 | NSIS `installMode` default is `perUser` | **corrected** to `currentUser` | v2.tauri.app/distribute/windows-installer |
| 12 | WebView2 bundling options and sizes; NSIS hooks | confirmed (added the PREINSTALL/POSTUNINSTALL hooks) | v2.tauri.app/distribute/windows-installer |
| 13 | Updater: mandatory signatures, endpoint variables, 204, Windows `installMode` passive/basicUi/quiet, app exits during install | confirmed | v2.tauri.app/plugin/updater |
| 14 | Capabilities: dir enabled by default, union/merge, iframe limitation on Linux/Android, threat-model exclusions | confirmed | v2.tauri.app/security/capabilities |
| 15 | Isolation: AES-GCM, per-launch keys, Windows ES-module limitation | confirmed; clarified that Tauri inlines automatically | v2.tauri.app/concept/inter-process-communication/isolation |
| 16 | Audit finding TAU2-040 = isolation key disclosure; audit dates; rewrites of iframe/scope validation | **unverified** (2.0 blog mentions only the mobile dev-server rewrite; PDF exists but text not extractable here) | v2.tauri.app/blog/tauri-20, audit PDF |
| 17 | `removeUnusedCommands` needs tauri 2.4+; size profile settings | confirmed | v2.tauri.app/concept/size |
| 18 | UniFFI: no cancellation; foreign side drives futures; issue #2448 open | confirmed | UniFFI futures page, GitHub #2448 |
| 19 | UniFFI methods on records/enums arrived in 0.30 | **corrected** to 0.31.0 | UniFFI CHANGELOG |
| 20 | `async_runtime = "tokio"` exists | **confirmed** (was marked unverified) | UniFFI CHANGELOG v0.32.0 (#2899) |
| 21 | `uniffi-bindgen-cs` v0.11.0 → UniFFI 0.31.0, .NET 8+ | confirmed | GitHub NordSecurity/uniffi-bindgen-cs |
| 22 | `objc2-service-management` 0.3.2 coverage of `SMAppService` methods | **confirmed** (was unverified) | docs.rs |
| 23 | Autostart plugin "uses `MacosLauncher::LaunchAgent`" | corrected: two variants (LaunchAgent/AppleScript), built on `auto-launch` 0.6, not `SMAppService` | docs.rs, plugins-workspace Cargo.toml |
| 24 | Hardened Runtime and notarization | added: Hardened Runtime is required for notarization | Apple: Hardened Runtime |
| 25 | Default `minimumSystemVersion` 10.13; `bundle.macOS.files` | confirmed; clarified that keys are relative to `Contents/` | v2.tauri.app/distribute/macos-application-bundle |
| 26 | Electron security checklist has 20 items | confirmed | electron/electron docs/tutorial/security.md |
| 27 | Slint royalty-free license covers desktop/mobile | confirmed (also web; excludes embedded) | slint.dev/pricing |
| 28 | Windows 10 horizon | added: WebView2 updates on Win10 22H2 until at least Oct 2028, no ESU needed | Microsoft Learn: Edge supported OSes |
| 29 | Omission: macOS 14+ other-app container consent (`NSAppDataUsageDescription`) | added | Apple docs; Apple forum thread 739602 |
| 30 | Omission: cloud placeholders (`FILE_ATTRIBUTE_RECALL_ON_*`, ubiquitous items) | added | Microsoft Learn file attribute constants; Apple docs |
| 31 | Omission: macOS App Management TCC; FDA implies it | added, marked unverified (third-party source only) | Lapcat Software |
| 32 | Third-party size/RAM benchmarks; TCC/FDA inheritance by `SMAppService` agents; Cmd+Q → `RunEvent::Exit` | remain **unverified** | — |
| 33 | All cited URLs | HTTP 200 checked for the GitHub audit PDF, CEF release tag, benchmark articles, Dioxus 0.7 docs and the Apple sandbox file-access page; other Tauri/Apple/Microsoft URLs were fetched successfully during verification | curl / WebFetch |
