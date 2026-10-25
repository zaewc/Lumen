// Lumen's download site (ADR-0038). Everything here is an enhancement: without
// JavaScript every link still goes to the latest GitHub release.
//
// With it, the page asks the GitHub API for the latest release, checks the
// answer's shape, and points the download buttons at this visitor's files.
// API data only ever reaches the page through textContent and validated hrefs.

const REPO = "zaewc/Lumen";
const API_URL = `https://api.github.com/repos/${REPO}/releases/latest`;
const RELEASES_URL = `https://github.com/${REPO}/releases`;
const LATEST_URL = `${RELEASES_URL}/latest`;
const DOWNLOAD_PREFIX = `${RELEASES_URL}/download/`;
const LANG_KEY = "lumen-site-lang";
const CACHE_KEY = "lumen-site-release";
const CACHE_MS = 10 * 60 * 1000;
const TIMEOUT_MS = 8000;

/** Desktop files by platform; the first one is the primary download. */
const PLATFORMS = {
  mac: { name: "macOS", suffixes: ["_universal.dmg"] },
  windows: { name: "Windows", suffixes: ["_x64-setup.exe"] },
  linux: { name: "Linux", suffixes: ["_amd64.deb", "_amd64.AppImage"] },
};

// --- Language ----------------------------------------------------------------

const root = document.documentElement;

/** Both languages of a short text, as spans; style.css shows the current one. */
function bi(ko, en) {
  const fragment = document.createDocumentFragment();
  for (const [lang, text] of [
    ["ko", ko],
    ["en", en],
  ]) {
    const span = document.createElement("span");
    span.lang = lang;
    span.textContent = text;
    fragment.append(span);
  }
  return fragment;
}

function applyLabels() {
  const lang = root.lang === "en" ? "en" : "ko";
  for (const element of document.querySelectorAll("[data-label-ko]")) {
    const label = element.getAttribute(`data-label-${lang}`);
    if (label !== null) {
      element.setAttribute("aria-label", label);
    }
  }
}

function setUpLanguageToggle() {
  const toggle = document.getElementById("lang-toggle");
  if (toggle === null) {
    return;
  }
  toggle.addEventListener("click", () => {
    const next = root.lang === "en" ? "ko" : "en";
    root.lang = next;
    applyLabels();
    try {
      window.localStorage.setItem(LANG_KEY, next);
    } catch {
      // The choice simply is not remembered.
    }
  });
  applyLabels();
}

// --- Platform ----------------------------------------------------------------

/** "mac", "windows", "linux", or null for anything else (phones, tablets, ChromeOS). */
function detectPlatform() {
  const ua = navigator.userAgent;
  if (/Android|iPhone|iPad|iPod|CrOS/i.test(ua)) {
    return null;
  }
  if (/Macintosh|Mac OS X/i.test(ua)) {
    // iPadOS asks for desktop sites as a Mac; a touch screen gives it away.
    return navigator.maxTouchPoints > 1 ? null : "mac";
  }
  if (/Windows/i.test(ua)) {
    return "windows";
  }
  if (/Linux|X11/i.test(ua)) {
    return "linux";
  }
  return null;
}

// --- Release data --------------------------------------------------------------

/**
 * The parts of a GitHub release this page uses, or null if the JSON does not
 * have the expected shape. Assets with unexpected names or URLs are dropped.
 */
function parseRelease(json) {
  if (typeof json !== "object" || json === null) {
    return null;
  }
  const { tag_name: tag, published_at: published, html_url: page, assets } = json;
  if (
    typeof tag !== "string" ||
    !/^v?\d{1,4}\.\d{1,4}\.\d{1,4}(?:-[0-9A-Za-z.]{1,40})?$/.test(tag)
  ) {
    return null;
  }
  if (!Array.isArray(assets)) {
    return null;
  }
  const date = typeof published === "string" ? new Date(published) : null;
  return {
    tag,
    version: tag.replace(/^v/, ""),
    date: date !== null && !Number.isNaN(date.getTime()) ? date : null,
    page: typeof page === "string" && page.startsWith(`${RELEASES_URL}/tag/`) ? page : LATEST_URL,
    assets: assets.map(parseAsset).filter((asset) => asset !== null),
  };
}

function parseAsset(asset) {
  if (typeof asset !== "object" || asset === null) {
    return null;
  }
  const { name, size, browser_download_url: url } = asset;
  if (typeof name !== "string" || !/^[A-Za-z0-9._+-]{1,120}$/.test(name)) {
    return null;
  }
  if (typeof url !== "string" || !url.startsWith(DOWNLOAD_PREFIX)) {
    return null;
  }
  let parsed;
  try {
    parsed = new URL(url);
  } catch {
    return null;
  }
  if (parsed.protocol !== "https:" || parsed.host !== "github.com") {
    return null;
  }
  return {
    name,
    url: parsed.href,
    size: typeof size === "number" && Number.isFinite(size) && size >= 0 ? size : null,
  };
}

function readCache() {
  try {
    const raw = window.sessionStorage.getItem(CACHE_KEY);
    if (raw === null) {
      return null;
    }
    const cached = JSON.parse(raw);
    if (typeof cached?.at !== "number" || Date.now() - cached.at > CACHE_MS) {
      return null;
    }
    if (cached.state === "none") {
      return { state: "none" };
    }
    const release = cached.state === "ok" ? parseRelease(cached.json) : null;
    return release === null ? null : { state: "ok", release };
  } catch {
    return null;
  }
}

function writeCache(state, json) {
  try {
    window.sessionStorage.setItem(CACHE_KEY, JSON.stringify({ at: Date.now(), state, json }));
  } catch {
    // Not cached; the next visit asks again.
  }
}

/** { state: "ok", release } | { state: "none" } | { state: "error", limited } */
async function loadRelease() {
  const cached = readCache();
  if (cached !== null) {
    return cached;
  }
  const controller = new AbortController();
  const timer = window.setTimeout(() => controller.abort(), TIMEOUT_MS);
  try {
    const response = await fetch(API_URL, {
      headers: { Accept: "application/vnd.github+json" },
      credentials: "omit",
      referrerPolicy: "no-referrer",
      signal: controller.signal,
    });
    if (response.status === 404) {
      writeCache("none", null);
      return { state: "none" };
    }
    if (!response.ok) {
      const limited =
        response.status === 429 ||
        (response.status === 403 && response.headers.get("x-ratelimit-remaining") === "0");
      return { state: "error", limited };
    }
    const json = await response.json();
    const release = parseRelease(json);
    if (release === null) {
      return { state: "error", limited: false };
    }
    writeCache("ok", json);
    return { state: "ok", release };
  } catch {
    return { state: "error", limited: false };
  } finally {
    window.clearTimeout(timer);
  }
}

// --- Rendering -----------------------------------------------------------------

function formatSize(bytes) {
  const [value, unit] =
    bytes >= 1_000_000
      ? [bytes / 1_000_000, "MB"]
      : bytes >= 1000
        ? [bytes / 1000, "KB"]
        : [bytes, "B"];
  const digits = unit !== "B" && value < 10 ? 1 : 0;
  return `${value.toFixed(digits)} ${unit}`;
}

function formatDate(date, locale) {
  return new Intl.DateTimeFormat(locale, { dateStyle: "long" }).format(date);
}

function findAsset(release, suffix) {
  return (
    release.assets.find(
      (asset) => asset.name.startsWith("Lumen_") && asset.name.endsWith(suffix),
    ) ?? null
  );
}

function replaceContent(element, ...nodes) {
  if (element !== null) {
    element.replaceChildren(...nodes);
  }
}

function link(href, content) {
  const anchor = document.createElement("a");
  anchor.href = href;
  anchor.append(content);
  return anchor;
}

function separator() {
  return document.createTextNode(" · ");
}

/** Version and date, in both languages. */
function versionLine(release) {
  const fragment = document.createDocumentFragment();
  const version = document.createElement("span");
  version.className = "version";
  version.textContent = release.tag;
  fragment.append(version);
  if (release.date !== null) {
    fragment.append(
      separator(),
      bi(formatDate(release.date, "ko-KR"), formatDate(release.date, "en-US")),
    );
  }
  return fragment;
}

function renderPlatforms(release, platform) {
  for (const row of document.querySelectorAll(".platform[data-os]")) {
    const current = row.querySelector(".current");
    if (current !== null) {
      current.hidden = row.getAttribute("data-os") !== platform;
    }
  }
  if (release === null) {
    return;
  }
  for (const button of document.querySelectorAll("a[data-asset]")) {
    const asset = findAsset(release, button.getAttribute("data-asset") ?? "");
    if (asset === null) {
      button.href = release.page;
      continue;
    }
    button.href = asset.url;
    const size = button.querySelector("[data-size]");
    if (size !== null && asset.size !== null) {
      size.textContent = formatSize(asset.size);
    }
  }
}

function renderFileList(release) {
  const container = document.getElementById("file-list");
  if (container === null) {
    return;
  }
  const table = document.createElement("table");
  const head = table.createTHead().insertRow();
  const nameHead = document.createElement("th");
  nameHead.scope = "col";
  nameHead.append(bi("파일", "File"));
  const sizeHead = document.createElement("th");
  sizeHead.scope = "col";
  sizeHead.className = "num";
  sizeHead.append(bi("크기", "Size"));
  head.append(nameHead, sizeHead);
  const body = table.createTBody();
  for (const asset of release.assets) {
    const row = body.insertRow();
    const name = row.insertCell();
    const anchor = document.createElement("a");
    anchor.href = asset.url;
    anchor.textContent = asset.name;
    name.append(anchor);
    const size = row.insertCell();
    size.className = "num";
    size.textContent = asset.size === null ? "" : formatSize(asset.size);
  }
  container.replaceChildren(table);
}

function renderPrimary(result, platform) {
  const button = document.getElementById("primary-download");
  const label = document.getElementById("primary-label");
  const meta = document.getElementById("primary-meta");
  const other = document.getElementById("other-platforms");
  if (button === null || label === null || meta === null) {
    return;
  }
  const info = platform === null ? null : PLATFORMS[platform];

  if (result.state === "none") {
    button.href = RELEASES_URL;
    replaceContent(label, bi("GitHub에서 보기", "View on GitHub"));
    replaceContent(
      meta,
      bi(
        "첫 베타를 준비하고 있습니다. 공개되면 이곳에서 내려받을 수 있습니다.",
        "The first beta is coming soon. It will be available here.",
      ),
    );
    return;
  }

  if (result.state === "error") {
    button.href = LATEST_URL;
    replaceContent(label, bi("GitHub에서 다운로드", "Download from GitHub"));
    replaceContent(
      meta,
      bi("최신 버전 정보를 불러오지 못했습니다.", "Could not load the latest version."),
    );
    return;
  }

  const { release } = result;
  const asset = info === null ? null : findAsset(release, info.suffixes[0]);
  const line = versionLine(release);
  if (info === null || asset === null) {
    button.href = "#download";
    replaceContent(label, bi("다운로드", "Download"));
    replaceContent(meta, line);
    return;
  }
  button.href = asset.url;
  replaceContent(label, bi(`${info.name}용 다운로드`, `Download for ${info.name}`));
  const extension = asset.name.slice(asset.name.lastIndexOf("."));
  line.append(separator(), extension);
  if (asset.size !== null) {
    line.append(separator(), formatSize(asset.size));
  }
  replaceContent(meta, line);
  if (other !== null) {
    other.hidden = false;
  }
}

function renderStatus(result) {
  const status = document.getElementById("release-status");
  if (status === null) {
    return;
  }
  if (result.state === "ok") {
    const line = versionLine(result.release);
    line.append(separator(), link(result.release.page, bi("릴리스 노트", "Release notes")));
    status.replaceChildren(line);
    return;
  }
  if (result.state === "none") {
    status.replaceChildren(
      bi("첫 베타를 준비하고 있습니다. ", "The first beta is coming soon. "),
      link(RELEASES_URL, bi("GitHub 릴리스", "GitHub releases")),
    );
    return;
  }
  status.replaceChildren(
    result.limited
      ? bi(
          "GitHub 요청 한도에 걸려 최신 버전을 확인하지 못했습니다. ",
          "GitHub's request limit was reached, so the latest version could not be checked. ",
        )
      : bi("최신 버전을 확인하지 못했습니다. ", "Could not check the latest version. "),
    link(LATEST_URL, bi("GitHub에서 내려받기", "Download from GitHub")),
  );
}

function renderVersionTexts(release, platform) {
  if (release !== null) {
    for (const element of document.querySelectorAll("[data-version]")) {
      element.textContent = release.version;
    }
    const footer = document.getElementById("footer-version");
    if (footer !== null) {
      footer.textContent = `${release.tag} `;
    }
  }
  const attest = document.getElementById("attest-file");
  if (attest !== null && platform !== null) {
    const suffix = PLATFORMS[platform].suffixes[0];
    const asset = release === null ? null : findAsset(release, suffix);
    attest.textContent = asset?.name ?? `Lumen_${release?.version ?? "<version>"}${suffix}`;
  }
}

function render(result, platform) {
  const release = result.state === "ok" ? result.release : null;
  renderPrimary(result, platform);
  renderStatus(result);
  renderPlatforms(release, platform);
  renderVersionTexts(release, platform);
  if (release !== null) {
    renderFileList(release);
  } else {
    replaceContent(
      document.getElementById("file-list"),
      link(RELEASES_URL, bi("GitHub 릴리스에서 모든 파일 보기", "See every file on GitHub")),
    );
  }
  for (const anchor of document.querySelectorAll("a[data-sums]")) {
    anchor.href =
      release === null
        ? RELEASES_URL
        : (findExact(release, "SHA256SUMS")?.url ?? `${LATEST_URL}/download/SHA256SUMS`);
  }
}

function findExact(release, name) {
  return release.assets.find((asset) => asset.name === name) ?? null;
}

// --- Disclosure and copy ---------------------------------------------------------

/** Opens the <details> a link points to (or, for the install guide, this platform's). */
function openTarget(hash, platform) {
  if (hash.length < 2) {
    return;
  }
  const target = document.getElementById(decodeURIComponent(hash.slice(1)));
  if (target === null) {
    return;
  }
  if (target instanceof HTMLDetailsElement) {
    target.open = true;
  }
  const inside = target.closest("details");
  if (inside !== null) {
    inside.open = true;
  }
  if (target.id === "install" && platform !== null) {
    const guide = target.querySelector(`details[data-os="${platform}"]`);
    if (guide instanceof HTMLDetailsElement) {
      guide.open = true;
    }
  }
}

function setUpDisclosure(platform) {
  document.addEventListener("click", (event) => {
    const anchor = event.target instanceof Element ? event.target.closest('a[href^="#"]') : null;
    if (anchor !== null) {
      openTarget(anchor.getAttribute("href") ?? "", platform);
    }
  });
  openTarget(window.location.hash, platform);
}

function setUpCopyButtons() {
  for (const button of document.querySelectorAll("button[data-copy]")) {
    button.addEventListener("click", async () => {
      const source = document.getElementById(button.getAttribute("data-copy") ?? "");
      if (
        source === null ||
        navigator.clipboard === undefined ||
        button.hasAttribute("data-copied")
      ) {
        return;
      }
      try {
        await navigator.clipboard.writeText(source.textContent ?? "");
      } catch {
        return;
      }
      const idle = Array.from(button.childNodes);
      button.setAttribute("data-copied", "");
      button.replaceChildren(bi("복사됨", "Copied"));
      window.setTimeout(() => {
        button.removeAttribute("data-copied");
        button.replaceChildren(...idle);
      }, 1500);
    });
  }
}

// --- Start ---------------------------------------------------------------------

const platform = detectPlatform();
setUpLanguageToggle();
setUpDisclosure(platform);
setUpCopyButtons();
renderPlatforms(null, platform);
renderVersionTexts(null, platform);
loadRelease().then((result) => render(result, platform));
