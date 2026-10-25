import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { loadRelease, parseAsset, parseRelease } from "./release";

const prefix = "https://github.com/zaewc/Lumen/releases/download/";
const asset = {
  name: "Lumen_0.1.0_universal.dmg",
  size: 42,
  browser_download_url: `${prefix}v0.1.0/Lumen_0.1.0_universal.dmg`,
};
const release = {
  tag_name: "v0.1.0",
  published_at: "2026-10-24T00:00:00Z",
  html_url: "https://github.com/zaewc/Lumen/releases/tag/v0.1.0",
  assets: [asset],
};

describe("untrusted release metadata", () => {
  it.each([
    "https://evil.example/Lumen.dmg",
    "https://github.com.evil.example/zaewc/Lumen/releases/download/v0.1.0/a",
    `${prefix}../../../../other-repo/a`,
    `${prefix}%2e%2e/%2e%2e/%2e%2e/other-repo/a`,
    `${prefix}v0.1.0/a?redirect=elsewhere`,
    `${prefix}v0.1.0/a#fragment`,
    "javascript:alert(1)",
  ])("rejects asset URL %s", (url) => {
    expect(parseAsset({ ...asset, browser_download_url: url })).toBeNull();
  });

  it("drops hostile assets while retaining a usable release", () => {
    const parsed = parseRelease({
      ...release,
      assets: [asset, { ...asset, name: "<script>.dmg" }, null],
    });
    expect(parsed?.assets).toEqual([
      { name: asset.name, url: asset.browser_download_url, size: 42 },
    ]);
  });

  it("rejects malformed envelopes and keeps invalid optional fields out of the UI", () => {
    expect(parseRelease({ ...release, assets: {} })).toBeNull();
    expect(parseRelease({ ...release, tag_name: "<script>" })).toBeNull();
    const parsed = parseRelease({
      ...release,
      published_at: "not a date",
      html_url: "https://evil.example",
    });
    expect(parsed?.date).toBeNull();
    expect(parsed?.page).toBe("https://github.com/zaewc/Lumen/releases/latest");
    expect(parseAsset({ ...asset, size: -1 })?.size).toBeNull();
  });
});

describe("release lookup and session cache", () => {
  const values = new Map<string, string>();
  const request = vi.fn<typeof fetch>();

  beforeEach(() => {
    values.clear();
    request.mockReset();
    vi.stubGlobal("fetch", request);
    vi.stubGlobal("window", {
      sessionStorage: {
        getItem: (key: string) => values.get(key) ?? null,
        setItem: (key: string, value: string) => values.set(key, value),
      },
      setTimeout,
      clearTimeout,
    });
  });
  afterEach(() => vi.unstubAllGlobals());

  it("caches a successful response and sends neither credentials nor a referrer", async () => {
    request.mockResolvedValue(new Response(JSON.stringify(release)));
    expect((await loadRelease()).state).toBe("ok");
    expect((await loadRelease()).state).toBe("ok");
    expect(request).toHaveBeenCalledTimes(1);
    expect(request.mock.calls[0]?.[1]).toMatchObject({
      credentials: "omit",
      referrerPolicy: "no-referrer",
    });
  });

  it.each([Date.now() - 600_001, Date.now() + 60_000])(
    "refetches an expired or future-dated cache (%s)",
    async (at) => {
      values.set("lumen-site-release", JSON.stringify({ at, state: "ok", json: release }));
      request.mockResolvedValue(new Response(null, { status: 404 }));
      expect(await loadRelease()).toEqual({ state: "none" });
      expect(request).toHaveBeenCalledTimes(1);
    },
  );

  it("revalidates cached releases", async () => {
    values.set(
      "lumen-site-release",
      JSON.stringify({ at: Date.now(), state: "ok", json: { ...release, tag_name: "bad" } }),
    );
    request.mockResolvedValue(new Response(null, { status: 404 }));
    expect(await loadRelease()).toEqual({ state: "none" });
    expect(request).toHaveBeenCalledTimes(1);
  });

  it.each([403, 429])("reports rate limiting (%s)", async (status) => {
    request.mockResolvedValue(
      new Response(null, { status, headers: { "x-ratelimit-remaining": "0" } }),
    );
    expect(await loadRelease()).toEqual({ state: "error", limited: true });
  });

  it("falls back safely when the response is not JSON", async () => {
    request.mockResolvedValue(new Response("<html>error</html>"));
    expect(await loadRelease()).toEqual({ state: "error", limited: false });
  });
});
