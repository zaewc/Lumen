import { describe, expect, it } from "vitest";
import { lensPixels } from "./glass-map";

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
