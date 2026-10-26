// Tauri's isolation pattern (ADR-0004): every IPC message from the webview
// passes through this sandboxed frame before the host sees it. Only Lumen's
// own commands get through; anything else is dropped here.
// Keep in step with build.rs, capabilities/main.json and lib.rs; a test in
// lib.rs fails when they differ. A command missing here never reaches the
// host, and its call never answers.
const allowed = new Set([
  "scan",
  "assess",
  "plan",
  "execute",
  "held",
  "restore",
  "finalize",
  "ai_settings",
  "set_ai_review",
  "set_ai_key",
  "disk",
  "open_full_disk_access",
]);

window.__TAURI_ISOLATION_HOOK__ = (payload) => {
  if (payload && allowed.has(payload.cmd)) {
    return payload;
  }
  return null;
};
