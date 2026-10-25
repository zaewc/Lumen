// GitHub release metadata is untrusted, including when read from the session cache.
import { API_URL, LATEST_URL, RELEASES_URL } from "../data/release";
import { z } from "../lib/zod";

const DOWNLOAD_PREFIX = `${RELEASES_URL}/download/`;
const CACHE_KEY = "lumen-site-release";
const CACHE_MS = 10 * 60 * 1000;
const TIMEOUT_MS = 8000;

export interface Asset {
  readonly name: string;
  readonly url: string;
  readonly size: number | null;
}

export interface Release {
  readonly tag: string;
  readonly version: string;
  readonly date: Date | null;
  readonly page: string;
  readonly assets: readonly Asset[];
}

export type ReleaseResult =
  | { readonly state: "ok"; readonly release: Release }
  | { readonly state: "none" }
  | { readonly state: "error"; readonly limited: boolean };

// Canonicalize before checking the repository prefix: URL normalizes ../ and
// encoded dot segments, which must not escape the trusted release location.
const repositoryUrl = (prefix: string) =>
  z
    .url()
    .transform((value) => new URL(value).href)
    .refine((value) => value.startsWith(prefix) && !new URL(value).search && !new URL(value).hash);

const assetSchema = z.object({
  name: z.string().regex(/^[A-Za-z0-9._+-]{1,120}$/),
  browser_download_url: repositoryUrl(DOWNLOAD_PREFIX),
  size: z.number().finite().nonnegative().nullable().catch(null),
});

const releaseSchema = z.object({
  tag_name: z.string().regex(/^v?\d{1,4}\.\d{1,4}\.\d{1,4}(?:-[0-9A-Za-z.]{1,40})?$/),
  published_at: z.string().nullable().catch(null),
  html_url: repositoryUrl(`${RELEASES_URL}/tag/`).catch(LATEST_URL),
  assets: z.array(z.unknown()),
});

export function parseRelease(json: unknown): Release | null {
  const parsed = releaseSchema.safeParse(json);
  if (!parsed.success) {
    return null;
  }
  const value = parsed.data;
  const date = value.published_at === null ? null : new Date(value.published_at);
  return {
    tag: value.tag_name,
    version: value.tag_name.replace(/^v/, ""),
    date: date !== null && !Number.isNaN(date.getTime()) ? date : null,
    page: value.html_url,
    assets: value.assets.map(parseAsset).filter((asset) => asset !== null),
  };
}

export function parseAsset(json: unknown): Asset | null {
  const parsed = assetSchema.safeParse(json);
  return parsed.success
    ? { name: parsed.data.name, url: parsed.data.browser_download_url, size: parsed.data.size }
    : null;
}

const cacheSchema = z.discriminatedUnion("state", [
  z.object({ state: z.literal("none"), at: z.number().finite().nonnegative() }),
  z.object({ state: z.literal("ok"), at: z.number().finite().nonnegative(), json: z.unknown() }),
]);

function readCache(): ReleaseResult | null {
  try {
    const raw = window.sessionStorage.getItem(CACHE_KEY);
    if (raw === null) {
      return null;
    }
    const json: unknown = JSON.parse(raw);
    const parsed = cacheSchema.safeParse(json);
    if (!parsed.success) {
      return null;
    }
    const cached = parsed.data;
    const age = Date.now() - cached.at;
    if (age < 0 || age > CACHE_MS) {
      return null;
    }
    if (cached.state === "none") {
      return { state: "none" };
    }
    const release = parseRelease(cached.json);
    return release === null ? null : { state: "ok", release };
  } catch {
    return null;
  }
}

function writeCache(state: "ok" | "none", json: unknown): void {
  try {
    window.sessionStorage.setItem(CACHE_KEY, JSON.stringify({ at: Date.now(), state, json }));
  } catch {
    // Storage is optional; the next visit can ask GitHub again.
  }
}

export async function loadRelease(): Promise<ReleaseResult> {
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
      return {
        state: "error",
        limited:
          response.status === 429 ||
          (response.status === 403 && response.headers.get("x-ratelimit-remaining") === "0"),
      };
    }
    const json: unknown = await response.json();
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

export function findAsset(release: Release, suffix: string): Asset | null {
  return (
    release.assets.find(
      (asset) => asset.name.startsWith("Lumen_") && asset.name.endsWith(suffix),
    ) ?? null
  );
}

export function findExact(release: Release, name: string): Asset | null {
  return release.assets.find((asset) => asset.name === name) ?? null;
}

export function formatSize(bytes: number): string {
  const [value, unit] =
    bytes >= 1_000_000
      ? [bytes / 1_000_000, "MB"]
      : bytes >= 1000
        ? [bytes / 1000, "KB"]
        : [bytes, "B"];
  return `${value.toFixed(unit !== "B" && value < 10 ? 1 : 0)} ${unit}`;
}
