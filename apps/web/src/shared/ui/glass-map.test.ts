import { describe, expect, it } from "vitest";
import { bendScale, type Engine, engineOf, lensPixels, squeezeMatrix } from "./glass-map";

describe("measured inverse lens", () => {
  it("leaves the label's centre and the outer sampling boundary neutral", () => {
    const field = lensPixels(240, 88, 44);
    expect(field).not.toBeNull();
    const pixel = (x: number, y: number) => [
      ...(field?.data.slice((y * 240 + x) * 4, (y * 240 + x) * 4 + 4) ?? []),
    ];
    expect(pixel(120, 44)).toEqual([128, 128, 0, 255]);
    expect(pixel(0, 44)).toEqual([128, 128, 0, 255]);
    expect(pixel(120, 0)).toEqual([128, 128, 0, 255]);
    expect(field?.scale).toBeGreaterThan(0);
  });

  it("bends opposite edges inward without translating the whole field", () => {
    const field = lensPixels(240, 88, 44);
    const channel = (x: number, y: number, c: number) => field?.data[(y * 240 + x) * 4 + c] ?? 128;
    expect(channel(8, 44, 0)).toBeGreaterThan(128);
    expect(channel(231, 44, 0)).toBeLessThan(128);
    expect(channel(120, 8, 1)).toBeGreaterThan(128);
    expect(channel(120, 79, 1)).toBeLessThan(128);
    expect(channel(8, 44, 0) + channel(231, 44, 0)).toBeCloseTo(255, 0);
  });

  it("rejects unbounded or non-finite allocation requests", () => {
    expect(lensPixels(Number.NaN, 88, 44)).toBeNull();
    expect(lensPixels(10_000, 88, 44)).toBeNull();
    expect(lensPixels(240, 10_000, 44)).toBeNull();
    expect(lensPixels(240.5, 88, 44)).toBeNull();
  });
});

describe("rim light", () => {
  const field = lensPixels(240, 88, 44, { specular: 0.6, bezel: 0.8, spread: 1 });
  const blue = (x: number, y: number) => field?.data[(y * 240 + x) * 4 + 2] ?? -1;

  it("lights the rim facing the light and leaves the centre dark", () => {
    expect(blue(120, 0)).toBeGreaterThan(blue(120, 87));
    expect(blue(120, 87)).toBeGreaterThan(0);
    expect(blue(120, 44)).toBe(0);
  });

  it("stays dark outside the rounded shape", () => {
    expect(blue(0, 0)).toBe(0);
    expect(blue(239, 87)).toBe(0);
  });
});

describe("fold limit", () => {
  it("bends by at most the measured limit without repeating content", () => {
    const width = 189;
    const field = lensPixels(width, 92, 46, { bezel: 0.8, spread: 1 });
    expect(field).not.toBeNull();
    const limit = field?.unfolded ?? 0;
    for (const y of [10, 46, 80]) {
      let previous = Number.NEGATIVE_INFINITY;
      for (let x = 0; x < width; x += 1) {
        const value = ((field?.data[(y * width + x) * 4] ?? 128) - 128) / 127;
        const sample = x + limit * value;
        expect(sample).toBeGreaterThanOrEqual(previous - 1e-6);
        previous = sample;
      }
    }
  });

  it("allows a larger bend with the linear encoding", () => {
    const saturated = lensPixels(189, 92, 46, { bezel: 0.8 });
    const linear = lensPixels(189, 92, 46, { bezel: 0.8, spread: 1 });
    expect(linear?.unfolded ?? 0).toBeGreaterThan(saturated?.unfolded ?? 0);
  });
});

describe("engine detection", () => {
  const cases: [string, readonly string[], Engine | null][] = [
    [
      "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/141.0.0.0 Safari/537.36 Edg/141.0.0.0",
      ["Chromium", "Microsoft Edge WebView2"],
      "blink",
    ],
    [
      "Mozilla/5.0 (Linux; Android 15) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/141.0.0.0 Mobile Safari/537.36",
      [],
      "blink",
    ],
    [
      "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko)",
      [],
      "webkit",
    ],
    [
      "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/26.0 Safari/605.1.15",
      [],
      "webkit",
    ],
    [
      "Mozilla/5.0 (iPhone; CPU iPhone OS 26_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) CriOS/141.0 Mobile/15E148 Safari/604.1",
      [],
      "webkit",
    ],
    [
      "Mozilla/5.0 (iPhone; CPU iPhone OS 26_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) FxiOS/141.0 Mobile/15E148 Safari/605.1.15",
      [],
      "webkit",
    ],
    [
      "Mozilla/5.0 (X11; Ubuntu; Linux x86_64) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Safari/605.1.15",
      [],
      "webkit",
    ],
    [
      "Mozilla/5.0 (Macintosh; Intel Mac OS X 10.15; rv:159.0) Gecko/20100101 Firefox/159.0",
      [],
      "gecko",
    ],
    ["Mozilla/5.0 (compatible; UnknownEngine/1.0)", [], null],
  ];
  it.each(cases)("%s", (agent, brands, engine) => {
    expect(engineOf(agent, brands)).toBe(engine);
  });
});

describe("per-engine displacement scale", () => {
  // The measured conversion of an objectBoundingBox `scale` into CSS px per axis;
  // map values reach ±0.5 around neutral, so the offset is half of it.
  function offsets(engine: Engine, width: number, height: number, px: number) {
    const { scale, squeeze } = bendScale(engine, px, width, height);
    const [kx, ky] = squeeze ?? [1, 1];
    switch (engine) {
      case "blink":
        return [(scale * width * kx) / 2, (scale * width * ky) / 2];
      case "webkit":
        return [(scale * width * kx) / 2, (scale * height * ky) / 2];
      case "gecko": {
        const diagonal = Math.sqrt((width * width + height * height) / 2);
        return [(scale * diagonal * kx) / 2, (scale * diagonal * ky) / 2];
      }
    }
  }

  it.each(["blink", "webkit", "gecko"] as const)(
    "bends %s by the same px on both axes",
    (engine) => {
      for (const [width, height] of [
        [430, 72],
        [366, 48],
        [72, 430],
      ] as const) {
        const [x, y] = offsets(engine, width, height, 3.5);
        expect(x).toBeCloseTo(3.5, 9);
        expect(y).toBeCloseTo(3.5, 9);
      }
    },
  );

  it("squeezes only in WebKit, never above 1", () => {
    expect(bendScale("blink", 3, 430, 72).squeeze).toBeNull();
    expect(bendScale("gecko", 3, 430, 72).squeeze).toBeNull();
    const squeeze = bendScale("webkit", 3, 430, 72).squeeze;
    expect(squeeze?.[0]).toBeCloseTo(72 / 430, 9);
    expect(squeeze?.[1]).toBe(1);
  });

  it("returns no bend for unusable sizes", () => {
    expect(bendScale("blink", 3, 0, 72)).toEqual({ scale: 0, squeeze: null });
    expect(bendScale("webkit", Number.NaN, 430, 72)).toEqual({ scale: 0, squeeze: null });
    expect(bendScale("gecko", -1, 430, 72)).toEqual({ scale: 0, squeeze: null });
  });

  it("keeps neutral grey neutral when squeezing", () => {
    const values = squeezeMatrix([0.25, 1]).split(/\s+/).map(Number);
    expect(values).toHaveLength(20);
    const red = (v: number) => (values[0] ?? 0) * v + (values[4] ?? 0);
    const green = (v: number) => (values[6] ?? 0) * v + (values[9] ?? 0);
    expect(red(0.5)).toBeCloseTo(0.5, 6);
    expect(red(1)).toBeCloseTo(0.625, 6);
    expect(red(0)).toBeCloseTo(0.375, 6);
    expect(green(0.8)).toBeCloseTo(0.8, 6);
  });
});
