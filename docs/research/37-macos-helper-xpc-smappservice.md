# 37. A macOS launch daemon over XPC: SMAppService, peer code-signing requirements and bundling

- Date: 2026-11-09
- Issue: ADR-0063, roadmap 16.3b
- Method: the macOS SDK headers in Xcode (`xpc/connection.h`, `xpc/listener.h`,
  `ServiceManagement.framework/Headers/SMAppService.h`), `man launchd.plist`, Apple
  documentation and Apple DTS answers on the Developer Forums, and the Tauri 2 source
  (`tauri-bundler`, `tauri-utils`, `tauri-build`). Two claims were tested locally, as
  noted. Anything else unconfirmed is marked **unconfirmed**.

## XPC peer code-signing requirement

- `xpc_connection_set_peer_code_signing_requirement` is macOS 12 and later. The header
  says: "All messages received on this connection will be checked to ensure they come
  from a peer who satisfies the code signing requirement. For a listener connection,
  requests that do not satisfy the requirement are dropped. When a reply is expected
  on the connection and the peer does not satisfy the requirement
  `XPC_ERROR_PEER_CODE_SIGNING_REQUIREMENT` will be delivered instead of the reply."
  Calling it twice on one connection is a programming error.
- The check uses each received message's audit token, not the connection, so a reused
  PID cannot pass it (Apple DTS, forum thread 837286). Apple recommends it over
  checking code yourself, which has time-of-check problems (threads 773573, 700962).
- Whether an accepted peer inherits a legacy listener's requirement is not
  documented; the macOS 26 listener API says peers do not. **Lumen sets it on the
  listener and again on each accepted peer.** Tested: setting it on the peer as well
  does not trap.
- A client's requirement protects only what it receives, so a client should look the
  service up in the system domain only (`XPC_CONNECTION_MACH_SERVICE_PRIVILEGED`).
- `xpc_connection_get_euid` ("the EUID of the remote peer at the time the connection
  was made") is public; `xpc_connection_get_audit_token` is still not in any SDK
  header and is not used. `xpc_connection_get_pid` can go stale and is not used for
  decisions.
- Releasing a connection that was never resumed is a programming error XPC traps on
  (found in Lumen's tests as SIGTRAP); a connection whose requirement is rejected is
  cancelled and resumed first.
- The Developer ID requirement (TN3127): `anchor apple generic and certificate
  1[field.1.2.840.113635.100.6.2.6] exists and certificate
  leaf[field.1.2.840.113635.100.6.1.13] exists and certificate leaf[subject.OU] =
  "TEAM" and identifier "ID"`. The two fields mark the Developer ID intermediate and
  a Developer ID Application leaf, so Apple Development builds do not pass.

## SMAppService daemons

- The plist lives in the app's `Contents/Library/LaunchDaemons/` and is named with
  `.plist` in `SMAppService.daemon(plistName:)`. It uses `Label`, `BundleProgram`
  (a bundle-relative executable path, which survives moving the app; `Program` would
  need an absolute path) and `MachServices`. `AssociatedBundleIdentifiers` is not
  needed for plists inside the bundle. Daemons run as root.
- Apple DTS's walkthrough puts the executable in `Contents/MacOS`; a report of
  `Contents/Helpers` failing with "Codesigning failure loading plist" exists
  (thread 768592).
- `register()` shows no password dialog. The daemon is not bootstrapped until an
  administrator approves it in System Settings › General › Login Items; the app can
  open that pane with `openSystemSettingsLoginItems()`. Status values:
  `notRegistered`, `enabled`, `requiresApproval` (also after the person revokes
  consent), `notFound`. `unregister()` kills a running daemon and does not wait.
- The header: "Apps that use SMAppService APIs must be code signed" and "Apps that
  contain LaunchDaemons must be notarized"; a bundle in `/Applications` is
  recommended so the daemon can start before login. Development builds signed but
  not notarized worked in Apple DTS's walkthrough; how strictly notarization is
  enforced outside development is **unconfirmed**.
- After an update that changes the plist or the executable, the service must be
  registered again (the header recommends `unregister` first). **If the daemon's
  code-signing identifier changes between versions, launchd stops launching it**, and
  there is no way to tell launchd (Apple DTS, thread 795022). The helper's identifier
  is therefore fixed: `dev.lumen.desktop.helper`, embedded in an Info.plist section.
- During development, `sfltool dumpbtm` shows the background-items database and
  `sudo sfltool resetbtm` resets it.

## On-demand launch and idle exit

- With `KeepAlive` false (the default), launchd starts the daemon when a message
  arrives on its Mach service; the port survives the process, so an idle exit is
  invisible to clients. A daemon that advertises `MachServices` must check in with
  `xpc_connection_create_mach_service` and may not call `xpc_main`.
- launchd throttles a job that ran less than `ThrottleInterval` (10 seconds by
  default). Apple DTS: idle-exit after about a minute (thread 133915). **Lumen's
  helper exits after 60 seconds with no request and none in progress.**

## Bundling with Tauri 2

- `bundle.externalBin` copies `<name>-<target triple>` into `Contents/MacOS/` without
  the suffix; a universal build expects `<name>-universal-apple-darwin` as well as
  each architecture's file. It does not need the shell plugin.
- `bundle.macOS.files` copies files into `Contents/`, before the bundle is signed.
- Tauri signs nested binaries with the app's entitlements and no `--identifier`, so
  the identifier comes from an embedded Info.plist or else the file name. Tested: a
  binary linked with `-sectcreate __TEXT __info_plist` and signed again gets
  `Identifier=dev.lumen.desktop.helper`.
