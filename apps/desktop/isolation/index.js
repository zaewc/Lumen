// Tauri's isolation pattern (ADR-0004): every IPC message from the webview
// passes through this sandboxed frame before the host sees it. Only Lumen's
// own commands get through; anything else is dropped here.
const allowed = new Set(["scan", "assess", "plan", "execute", "held", "restore", "finalize"]);

window.__TAURI_ISOLATION_HOOK__ = (payload) => {
  if (payload && allowed.has(payload.cmd)) {
    return payload;
  }
  return null;
};
