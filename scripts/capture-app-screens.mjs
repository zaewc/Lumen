// Retake the website and README captures from the real UI, using sample data only.
// Node 24, cwebp and an isolated Chrome CDP session; see site-illustrations.md.
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdir, mkdtemp, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const repo = fileURLToPath(new URL("../", import.meta.url));
const origin = process.env.LUMEN_CAPTURE_ORIGIN ?? "http://127.0.0.1:5198";
const debuggerOrigin = process.env.LUMEN_CAPTURE_CDP ?? "http://127.0.0.1:9342";
const local = (value) => ["localhost", "127.0.0.1", "[::1]"].includes(new URL(value).hostname);
assert.ok(local(origin) && local(debuggerOrigin), "Use local fixture and browser servers only");
const target = await (
  await fetch(`${debuggerOrigin}/json/new?about:blank`, { method: "PUT" })
).json();
const ws = new WebSocket(target.webSocketDebuggerUrl);
await new Promise((resolve) => ws.addEventListener("open", resolve, { once: true }));
let next = 0;
const pending = new Map();
const errors = [];
const requests = new Set();
let injected = 0;

function send(method, params = {}) {
  return new Promise((resolve, reject) => {
    const id = ++next;
    const timer = setTimeout(() => {
      pending.delete(id);
      reject(new Error(`CDP timed out: ${method}`));
    }, 30_000);
    pending.set(id, { resolve, reject, timer });
    ws.send(JSON.stringify({ id, method, params }));
  });
}

async function fixture(params) {
  const { requestId } = params;
  const response = await send("Fetch.getResponseBody", { requestId });
  const source = response.base64Encoded
    ? Buffer.from(response.body, "base64").toString("utf8")
    : response.body;
  // Only the browser's fixture transport changes. Never use the native host,
  // a keychain or a real AI key, and never rewrite the rendered interface.
  const pattern = /memoryTransport\(\{\s*pace:\s*700\s*\}\)/g;
  assert.equal([...source.matchAll(pattern)].length, 1, "Fixture bootstrap changed; review it");
  const body = source.replace(pattern, "memoryTransport({ pace: 0, builtinKey: true })");
  injected += 1;
  await send("Fetch.fulfillRequest", {
    requestId,
    responseCode: 200,
    responseHeaders: [{ name: "Content-Type", value: "application/javascript" }],
    body: Buffer.from(body).toString("base64"),
  });
}

ws.addEventListener("message", (event) => {
  const message = JSON.parse(event.data);
  if (message.id) {
    const waiting = pending.get(message.id);
    if (waiting) {
      clearTimeout(waiting.timer);
      pending.delete(message.id);
      if (message.error) waiting.reject(new Error(JSON.stringify(message.error)));
      else waiting.resolve(message.result);
    }
  }
  if (message.method === "Fetch.requestPaused") {
    fixture(message.params).catch((error) => errors.push(String(error)));
  }
  if (message.method === "Runtime.exceptionThrown") {
    errors.push(message.params.exceptionDetails.text);
  }
  if (message.method === "Network.requestWillBeSent") requests.add(message.params.request.url);
});

async function evaluate(expression) {
  const result = await send("Runtime.evaluate", {
    expression,
    returnByValue: true,
    awaitPromise: true,
  });
  assert.equal(result.exceptionDetails, undefined, JSON.stringify(result.exceptionDetails));
  return result.result.value;
}

async function until(expression) {
  for (let attempt = 0; attempt < 150; attempt += 1) {
    assert.deepEqual(errors, []);
    if (await evaluate(expression)) return;
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  throw new Error(`Timed out: ${expression}`);
}

async function click(expression) {
  assert.equal(await evaluate(`(() => { const element = ${expression};
    if (!element) return false; element.click(); return true; })()`), true, expression);
}

const output = await mkdtemp(join(tmpdir(), "lumen-app-captures-"));
const readme = join(repo, "docs/assets/readme");
const site = join(repo, "apps/site/public/screens");
await Promise.all([mkdir(readme, { recursive: true }), mkdir(site, { recursive: true })]);

async function capture(name, locale, theme) {
  await evaluate("document.fonts.ready.then(() => true)");
  assert.equal(await evaluate("document.documentElement.scrollWidth > innerWidth"), false);
  assert.equal(await evaluate("document.querySelector('nav').getBoundingClientRect().height < 100"), true);
  const image = await send("Page.captureScreenshot", { format: "png", captureBeyondViewport: false });
  const png = Buffer.from(image.data, "base64");
  const temporary = join(output, `${name}-${locale}-${theme}.png`);
  await writeFile(temporary, png);
  if (locale === "en") await writeFile(join(readme, `${name}-${theme}.png`), png);
  if (theme === "light" && name !== "overview") {
    execFileSync("cwebp", [
      "-quiet", "-q", "92", "-m", "6", "-resize", "1600", "0",
      temporary, "-o", join(site, `${name}-${locale}.webp`),
    ]);
  }
  console.log(`${locale} ${theme}: ${name}`);
}

let preload;
try {
  await send("Page.enable");
  await send("Runtime.enable");
  await send("Network.enable");
  await send("Fetch.enable", {
    patterns: [{ urlPattern: "*/src/app/main.tsx*", requestStage: "Response" }],
  });
  await send("Page.bringToFront");
  await send("Emulation.setTimezoneOverride", { timezoneId: "Asia/Seoul" });
  for (const locale of ["en", "ko"]) {
    for (const theme of ["light", "dark"]) {
      await send("Emulation.setDeviceMetricsOverride", {
        width: 1120, height: 800, deviceScaleFactor: 2, mobile: false,
      });
      if (preload) await send("Page.removeScriptToEvaluateOnNewDocument", { identifier: preload });
      preload = (await send("Page.addScriptToEvaluateOnNewDocument", {
        source: `localStorage.setItem("lumen.locale", ${JSON.stringify(locale)});
          const OriginalDate = Date;
          globalThis.Date = class extends OriginalDate {
            constructor(...args) { super(...(args.length ? args : ["2026-10-29T03:00:00Z"])); }
            static now() { return OriginalDate.parse("2026-10-29T03:00:00Z"); }
          };`,
      })).identifier;
      await send("Emulation.setEmulatedMedia", { features: [
        { name: "prefers-color-scheme", value: theme },
        { name: "prefers-reduced-motion", value: "reduce" },
      ] });
      await send("Page.navigate", { url: `${origin}/` });
      await until(`document.documentElement.lang === ${JSON.stringify(locale)} &&
        [...document.querySelectorAll('main button')].some(element =>
          ["Scan disk", "디스크 검사"].includes(element.textContent.trim()))`);
      await click(`[...document.querySelectorAll('main button')].find(element =>
        ["Scan disk", "디스크 검사"].includes(element.textContent.trim()))`);
      await until("document.querySelectorAll('[data-row]').length === 13");
      await capture("clean-up", locale, theme);

      await click("document.querySelectorAll('fieldset label')[1]");
      await until("document.querySelectorAll('[data-row]').length === 3");
      await click("[...document.querySelectorAll('[data-row]')].find(element => element.textContent.includes('OldEditor'))");
      await until("document.querySelector('dialog[open]')?.textContent.includes('OldEditor')");
      await capture("review", locale, theme);
      await send("Input.dispatchKeyEvent", { type: "keyDown", key: "Escape", code: "Escape", windowsVirtualKeyCode: 27 });
      await send("Input.dispatchKeyEvent", { type: "keyUp", key: "Escape", code: "Escape", windowsVirtualKeyCode: 27 });
      await until("document.querySelector('dialog[open]') === null");

      await click("document.querySelector('nav a[href=\"/\"]')");
      await until("document.querySelector('main h1')?.textContent === " + JSON.stringify(locale === "ko" ? "저장 공간" : "Storage"));
      await capture("overview", locale, theme);
      // Two held items fit in the desktop's minimum window height.
      await send("Emulation.setDeviceMetricsOverride", {
        width: 1120, height: 560, deviceScaleFactor: 2, mobile: false,
      });
      await click("document.querySelector('nav a[href=\"/quarantine\"]')");
      await until("document.querySelectorAll('main li[data-selected]').length === 2");
      await click("document.querySelector('main li input[type=checkbox]')");
      await until("document.querySelectorAll('main li[data-selected=true]').length === 1");
      await capture("quarantine", locale, theme);
    }
  }
  assert.equal(injected, 4);
  assert.deepEqual(errors, []);
  assert.ok([...requests].filter((url) => /^https?:/.test(url)).every(local), "No external requests");
  console.log(`Refreshed 8 README PNGs and 6 website WebPs. Originals: ${output}`);
} finally {
  await send("Page.close");
  ws.close();
}
