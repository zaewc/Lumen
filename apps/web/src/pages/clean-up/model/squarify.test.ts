import { describe, expect, it } from "vitest";
import { type Rect, squarify } from "./squarify";

const frame: Rect = { x: 0, y: 0, width: 6, height: 4 };

function area(rect: Rect): number {
  return rect.width * rect.height;
}

function overlaps(a: Rect, b: Rect): boolean {
  const e = 1e-9;
  return (
    a.x + a.width > b.x + e &&
    b.x + b.width > a.x + e &&
    a.y + a.height > b.y + e &&
    b.y + b.height > a.y + e
  );
}

describe("squarify", () => {
  // The worked example in Bruls, Huizing and van Wijk (2000).
  const weights = [6, 6, 4, 3, 2, 2, 1];
  const rects = squarify(weights, frame);

  it("gives each weight its share of the frame, in input order", () => {
    weights.forEach((weight, n) => {
      expect(area(rects[n] as Rect)).toBeCloseTo(weight, 9);
    });
  });

  it("places the largest first, as the paper does", () => {
    expect(rects[0]).toEqual({ x: 0, y: 0, width: 3, height: 2 });
    expect(rects[1]).toEqual({ x: 0, y: 2, width: 3, height: 2 });
  });

  it("tiles the frame without overlaps or gaps", () => {
    for (const [n, a] of rects.entries()) {
      expect(a.x).toBeGreaterThanOrEqual(0);
      expect(a.y).toBeGreaterThanOrEqual(0);
      expect(a.x + a.width).toBeLessThanOrEqual(6 + 1e-9);
      expect(a.y + a.height).toBeLessThanOrEqual(4 + 1e-9);
      for (const b of rects.slice(n + 1)) {
        expect(overlaps(a, b)).toBe(false);
      }
    }
    expect(rects.reduce((total, rect) => total + area(rect), 0)).toBeCloseTo(24, 9);
  });

  it("keeps the input order whatever the sizes", () => {
    const shuffled = squarify([1, 6, 2], frame);
    expect(area(shuffled[0] as Rect)).toBeCloseTo(24 / 9, 9);
    expect(area(shuffled[1] as Rect)).toBeCloseTo(16, 9);
    expect(shuffled[1]).toMatchObject({ x: 0, y: 0 });
  });

  it("gives nothing to empty weights or an empty frame", () => {
    expect(squarify([0, 2], frame)[0]).toEqual({ x: 0, y: 0, width: 0, height: 0 });
    expect(squarify([0, 0], frame).every((rect) => area(rect) === 0)).toBe(true);
    expect(squarify([1], { x: 2, y: 3, width: 0, height: 5 })).toEqual([
      { x: 2, y: 3, width: 0, height: 0 },
    ]);
    expect(squarify([], frame)).toEqual([]);
  });

  it("fills the frame with a single weight", () => {
    expect(squarify([5], { x: 1, y: 1, width: 10, height: 2 })).toEqual([
      { x: 1, y: 1, width: 10, height: 2 },
    ]);
  });
});
