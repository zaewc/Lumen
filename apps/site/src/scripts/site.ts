// Lumen's download site (ADR-0038). Everything here is an enhancement: without
// JavaScript every link still goes to GitHub's releases page.
//
// With it, the page asks the GitHub API for the latest release (release.ts)
// and points the download buttons at this visitor's files. API data only ever
// reaches the page through textContent and validated hrefs.

import { PLATFORMS, type Platform, type PlatformId, RELEASES_URL, REPO_URL } from "../data/release";
import { type ClientStrings, parseClientStrings } from "../i18n/client";
import {
  type Asset,
  findAsset,
  findExact,
  formatSize,
  loadRelease,
  type Release,
  type ReleaseResult,
} from "./release";

// --- Platform ----------------------------------------------------------------

/** The visitor's desktop platform, or null for anything else (phones, tablets, ChromeOS). */
function detectPlatform(): Platform | null {
  const ua = navigator.userAgent;
  let id: PlatformId | null = null;
  if (/Android|iPhone|iPad|iPod|CrOS/i.test(ua)) {
    id = null;
  } else if (/Macintosh|Mac OS X/i.test(ua)) {
    // iPadOS asks for desktop sites as a Mac; a touch screen gives it away.
    id = navigator.maxTouchPoints > 1 ? null : "mac";
  } else if (/Windows/i.test(ua)) {
    id = "windows";
  } else if (/Linux|X11/i.test(ua)) {
    id = "linux";
  }
  return PLATFORMS.find((platform) => platform.id === id) ?? null;
}

// --- Rendering -----------------------------------------------------------------

function formatDate(date: Date, locale: string): string {
  return new Intl.DateTimeFormat(locale, { dateStyle: "long" }).format(date);
}

function replaceContent(element: Element | null, ...nodes: (Node | string)[]): void {
  element?.replaceChildren(...nodes);
}

function link(href: string, text: string): HTMLAnchorElement {
  const anchor = document.createElement("a");
  anchor.href = href;
  anchor.textContent = text;
  return anchor;
}

const SEPARATOR = " · ";

function preferredAsset(release: Release, platform: Platform): Asset | null {
  for (const file of platform.files) {
    const asset = findAsset(release, file.suffix);
    if (asset !== null) {
      return asset;
    }
  }
  return null;
}

/** Version and date. */
function versionLine(release: Release, t: ClientStrings): DocumentFragment {
  const fragment = document.createDocumentFragment();
  const version = document.createElement("span");
  version.className = "version";
  version.textContent = release.tag;
  fragment.append(version);
  if (release.date !== null) {
    fragment.append(SEPARATOR, formatDate(release.date, t.dateLocale));
  }
  return fragment;
}

function renderPlatforms(release: Release | null, platform: Platform | null): void {
  for (const row of document.querySelectorAll<HTMLElement>(".platform[data-os]")) {
    const current = row.querySelector<HTMLElement>(".current");
    if (current !== null) {
      current.hidden = row.getAttribute("data-os") !== platform?.id;
    }
  }
  for (const button of document.querySelectorAll<HTMLAnchorElement>("a[data-asset]")) {
    const asset =
      release === null ? null : findAsset(release, button.getAttribute("data-asset") ?? "");
    if (asset === null) {
      button.href = release?.page ?? RELEASES_URL;
      continue;
    }
    button.href = asset.url;
    const size = button.querySelector("[data-size]");
    if (size !== null && asset.size !== null) {
      size.textContent = formatSize(asset.size);
    }
  }
}

function renderFileList(release: Release, t: ClientStrings): void {
  const container = document.getElementById("file-list");
  if (container === null) {
    return;
  }
  const table = document.createElement("table");
  const head = table.createTHead().insertRow();
  const nameHead = document.createElement("th");
  nameHead.scope = "col";
  nameHead.textContent = t.fileHeader;
  const sizeHead = document.createElement("th");
  sizeHead.scope = "col";
  sizeHead.className = "num";
  sizeHead.textContent = t.sizeHeader;
  head.append(nameHead, sizeHead);
  const body = table.createTBody();
  for (const asset of release.assets) {
    const row = body.insertRow();
    row.insertCell().append(link(asset.url, asset.name));
    const size = row.insertCell();
    size.className = "num";
    size.textContent = asset.size === null ? "" : formatSize(asset.size);
  }
  container.replaceChildren(table);
}

function renderPrimary(result: ReleaseResult, platform: Platform | null, t: ClientStrings): void {
  const button = document.querySelector<HTMLAnchorElement>("#primary-download");
  const label = document.getElementById("primary-label");
  const meta = document.getElementById("primary-meta");
  if (button === null || label === null || meta === null) {
    return;
  }

  switch (result.state) {
    case "none":
      button.href = REPO_URL;
      label.textContent = t.viewOnGitHub;
      meta.replaceChildren();
      return;
    case "error":
      button.href = RELEASES_URL;
      label.textContent = t.downloadFromGitHub;
      meta.textContent = t.loadFailed;
      return;
    case "ok":
      break;
    default:
      result satisfies never;
  }

  const { release } = result;
  const asset = platform === null ? null : preferredAsset(release, platform);
  const line = versionLine(release, t);
  if (platform === null || asset === null) {
    button.href = "#download";
    label.textContent = t.download;
    meta.replaceChildren(line);
    return;
  }
  button.href = asset.url;
  label.textContent = t.downloadFor.replace("{platform}", platform.name);
  line.append(SEPARATOR, asset.name.slice(asset.name.lastIndexOf(".")));
  if (asset.size !== null) {
    line.append(SEPARATOR, formatSize(asset.size));
  }
  meta.replaceChildren(line);
}

function renderStatus(result: ReleaseResult, t: ClientStrings): void {
  const status = document.getElementById("release-status");
  switch (result.state) {
    case "ok": {
      const line = versionLine(result.release, t);
      line.append(SEPARATOR, link(result.release.page, t.releaseNotes));
      replaceContent(status, line);
      return;
    }
    case "none":
      replaceContent(status, t.comingSoonShort, link(RELEASES_URL, t.githubReleases));
      return;
    case "error":
      replaceContent(
        status,
        result.limited ? t.rateLimited : t.checkFailed,
        link(RELEASES_URL, t.getFromGitHub),
      );
      return;
    default:
      result satisfies never;
  }
}

function renderVersionTexts(release: Release | null, platform: Platform | null): void {
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
    const { suffix } = platform.files[0];
    const asset = release === null ? null : preferredAsset(release, platform);
    attest.textContent = asset?.name ?? `Lumen_${release?.version ?? "<version>"}${suffix}`;
  }
}

function render(result: ReleaseResult, platform: Platform | null, t: ClientStrings): void {
  const release = result.state === "ok" ? result.release : null;
  renderPrimary(result, platform, t);
  renderStatus(result, t);
  renderPlatforms(release, platform);
  renderVersionTexts(release, platform);
  if (release !== null) {
    renderFileList(release, t);
  } else {
    replaceContent(document.getElementById("file-list"), link(RELEASES_URL, t.allFiles));
  }
  for (const anchor of document.querySelectorAll<HTMLAnchorElement>("a[data-sums]")) {
    anchor.href =
      release === null ? RELEASES_URL : (findExact(release, "SHA256SUMS")?.url ?? release.page);
  }
}

// --- Disclosure and copy ---------------------------------------------------------

/** Opens the <details> a link points to (or, for the install guide, this platform's). */
function openTarget(hash: string, platform: Platform | null): void {
  if (hash.length < 2) {
    return;
  }
  let id: string;
  try {
    id = decodeURIComponent(hash.slice(1));
  } catch {
    return;
  }
  const target = document.getElementById(id);
  if (target === null) {
    return;
  }
  if (target instanceof HTMLDetailsElement) {
    target.open = true;
  }
  for (let parent = target.parentElement; parent !== null; parent = parent.parentElement) {
    if (parent instanceof HTMLDetailsElement) {
      parent.open = true;
    }
  }
  if (target.id === "install" && platform !== null) {
    const guide = target.querySelector(`details[data-os="${platform.id}"]`);
    if (guide instanceof HTMLDetailsElement) {
      guide.open = true;
    }
  }
}

function setUpDisclosure(platform: Platform | null): void {
  document.addEventListener("click", (event) => {
    const anchor = event.target instanceof Element ? event.target.closest('a[href^="#"]') : null;
    if (anchor !== null) {
      openTarget(anchor.getAttribute("href") ?? "", platform);
    }
  });
  openTarget(window.location.hash, platform);
  window.addEventListener("hashchange", () => openTarget(window.location.hash, platform));
}

function setUpCopyButtons(t: ClientStrings): void {
  for (const button of document.querySelectorAll<HTMLButtonElement>("button[data-copy]")) {
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
      button.textContent = t.copied;
      window.setTimeout(() => {
        button.removeAttribute("data-copied");
        button.replaceChildren(...idle);
      }, 1500);
    });
  }
}

// --- Start ---------------------------------------------------------------------

const strings = parseClientStrings(document.body.getAttribute("data-strings") ?? undefined);
if (strings !== null) {
  const platform = detectPlatform();
  setUpDisclosure(platform);
  setUpCopyButtons(strings);
  renderPlatforms(null, platform);
  renderVersionTexts(null, platform);
  void loadRelease().then((result) => render(result, platform, strings));
}
