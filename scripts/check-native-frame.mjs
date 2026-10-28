// Run after `pnpm --filter @lumen/web build` on macOS with Xcode tools installed.
// No host transport, keychain, network requests or user-file operations are used.
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { createHash } from "node:crypto";
import { mkdtemp, readFile } from "node:fs/promises";
import { createServer } from "node:http";
import { tmpdir } from "node:os";
import { extname, join } from "node:path";
import { fileURLToPath } from "node:url";

assert.equal(process.platform, "darwin", "This check exercises WKWebView on macOS");
const repo = fileURLToPath(new URL("../", import.meta.url));
const dist = join(repo, "apps/web/dist");
const config = JSON.parse(
  await readFile(join(repo, "apps/desktop/src-tauri/tauri.conf.json"), "utf8"),
);
const csp = config.app.security.csp;
const responder =
  "addEventListener('message', e => { if(e.source===parent && e.data==='ping') parent.postMessage('pong','*') });";
const responderHash = createHash("sha256").update(responder).digest("base64");
assert.ok(!csp.includes("'unsafe-inline'"), "Production must retain its strict CSP");
const output = await mkdtemp(join(tmpdir(), "lumen-native-frame-"));
const run = (command, args) =>
  new Promise((resolve, reject) => {
    const child = spawn(command, args, { stdio: ["ignore", "pipe", "inherit"] });
    let stdout = "";
    child.stdout.on("data", (data) => {
      stdout += data;
    });
    child.on("error", reject);
    child.on("close", (code) =>
      code === 0 ? resolve(stdout.trim()) : reject(new Error(`${command}: ${code}`)),
    );
  });
await run("swiftc", [
  "-module-cache-path",
  join(output, "modules"),
  fileURLToPath(new URL("check-native-frame.swift", import.meta.url)),
  "-o",
  join(output, "probe"),
]);

const server = createServer(async (request, response) => {
  const path = new URL(request.url ?? "/", "http://localhost").pathname;
  response.setHeader("Content-Security-Policy", csp);
  if (path === "/isolation.html") {
    // Tauri's isolation protocol uses default-src none and hashed scripts.
    response.setHeader(
      "Content-Security-Policy",
      `default-src 'none'; script-src 'sha256-${responderHash}'`,
    );
    response.setHeader("Content-Type", "text/html");
    response.end(`<!doctype html><script>${responder}</script>`);
    return;
  }
  const file = /^\/(assets\/[\w.-]+|favicon\.png|icon\.png)$/.test(path)
    ? join(dist, path)
    : join(dist, "index.html");
  try {
    const data = await readFile(file);
    response.setHeader(
      "Content-Type",
      { ".html": "text/html", ".js": "text/javascript", ".css": "text/css", ".png": "image/png" }[
        extname(file)
      ] ?? "application/octet-stream",
    );
    response.end(data);
  } catch {
    response.writeHead(404);
    response.end();
  }
});
await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
try {
  const address = server.address();
  assert.ok(address && typeof address === "object");
  for (const theme of ["light", "dark"]) {
    for (const width of [1120, 420]) {
      const result = JSON.parse(
        await run(join(output, "probe"), [
          `http://127.0.0.1:${address.port}/settings`,
          String(width),
          theme,
          join(output, `${theme}-${width}.png`),
        ]),
      );
      assert.equal(result.display, "none", JSON.stringify(result));
      assert.equal(result.width, 0);
      assert.equal(result.height, 0);
      assert.equal(
        result.documentHeight,
        result.viewportHeight,
        "Frame must not add document scroll",
      );
      assert.equal(result.replied, true, "Hidden frame must still exchange messages");
      assert.ok(
        result.blocked.some((directive) => directive.startsWith("style-src")),
        "Inline hiding style must remain blocked",
      );
      console.log(`${theme} ${width}: hidden, no added scroll, messages exchanged`);
    }
  }
  console.log(`Native snapshots: ${output}`);
} finally {
  server.close();
}
