/**
 * Inverse lens field adapted from liquid-glass-design 0.3.2.
 * MIT License. Copyright (c) 2026.
 * Permission is hereby granted, free of charge, to any person obtaining a copy
 * of this software and associated documentation files (the "Software"), to deal
 * in the Software without restriction, including without limitation the rights
 * to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
 * copies of the Software, and to permit persons to whom the Software is
 * furnished to do so, subject to the following conditions:
 * The above copyright notice and this permission notice shall be included in all
 * copies or substantial portions of the Software.
 * THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
 * IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
 * FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
 * AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
 * LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
 * OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
 * SOFTWARE.
 */
// Lumen's additions below the adapted field (rim light, per-engine filter units and
// engine detection) are written from the measurements in
// docs/research/32-app-glass-optics.md; no third-party code is copied.

const clamp = (v: number, low: number, high: number) => Math.max(low, Math.min(high, v));
const smooth = (low: number, high: number, v: number) => {
  const t = clamp((v - low) / (high - low || 1), 0, 1);
  return t * t * (3 - 2 * t);
};

/** Shape of the bend and the rim light, in map pixels where a length is involved. */
export interface LensOptics {
  /** Curved band as a share of the half short side; the rest is flat and leaves text alone. */
  readonly bezel?: number;
  /** Rim light strength, 0–1; 0 leaves the blue channel empty. */
  readonly specular?: number;
  /** Width of the crisp rim line in map pixels; a softer glow reaches five times as far. */
  readonly rim?: number;
  /** Light direction in degrees, screen axes: 0 from the right, -90 from the top. */
  readonly light?: number;
  /** Encoding range as a share of the largest bend; below 1 saturates the outer rim. */
  readonly spread?: number;
}

/**
 * Pure, bounded inverse mapping: identity centre, curved thin bezel, neutral rim.
 * R/G encode the bend around 128; B is rim light (0 = none); alpha is opaque.
 */
export function lensPixels(width: number, height: number, radius: number, optics: LensOptics = {}) {
  if (
    ![width, height, radius].every(Number.isFinite) ||
    width < 24 ||
    height < 24 ||
    width > 1024 ||
    height > 256 ||
    !Number.isInteger(width) ||
    !Number.isInteger(height)
  ) {
    return null;
  }
  const r = clamp(radius, 0, Math.min(width, height) / 2);
  const share = clamp(optics.bezel ?? 0.62, 0.1, 1);
  const bezel = clamp(Math.min(width, height) * 0.5 * share, 4, 220);
  const specular = clamp(optics.specular ?? 0, 0, 1);
  const rim = clamp(optics.rim ?? 3, 0.5, 16);
  const angle = ((optics.light ?? -120) * Math.PI) / 180;
  const lx = Math.cos(angle);
  const ly = Math.sin(angle);
  const raw = new Float32Array(width * height * 2);
  const light = new Float32Array(width * height);
  let max = 0;
  for (let y = 0; y < height; y += 1) {
    for (let x = 0; x < width; x += 1) {
      const px = x + 0.5;
      const py = y + 0.5;
      const qx = Math.abs(px - width / 2) - Math.max(0, width / 2 - r);
      const qy = Math.abs(py - height / 2) - Math.max(0, height / 2 - r);
      const distance =
        Math.min(Math.max(qx, qy), 0) + Math.hypot(Math.max(qx, 0), Math.max(qy, 0)) - r;
      let dx = 0;
      let dy = 0;
      if (distance > -bezel) {
        const pull = smooth(-bezel, 0, Math.min(distance, 0));
        const shaped = smooth(0, 1, pull) ** 1.45;
        dx = (-(px - width / 2) / (width / 2)) * shaped * bezel;
        dy = (-(py - height / 2) / (height / 2)) * shaped * bezel;
        dx += Math.sin((px / width - 0.5) * Math.PI) * 0.06 * bezel * shaped;
        dy += Math.sin((py / height - 0.5) * Math.PI) * 0.06 * bezel * shaped;
      }
      const i = (y * width + x) * 2;
      raw[i] = dx;
      raw[i + 1] = dy;
      max = Math.max(max, Math.abs(dx), Math.abs(dy));
      if (specular > 0 && distance < 0.5) {
        // Outward normal of the rounded rectangle at this pixel.
        const sx = px < width / 2 ? -1 : 1;
        const sy = py < height / 2 ? -1 : 1;
        let nx = 0;
        let ny = 0;
        if (qx > 0 && qy > 0) {
          const length = Math.hypot(qx, qy) || 1;
          nx = (sx * qx) / length;
          ny = (sy * qy) / length;
        } else if (qx > qy) {
          nx = sx;
        } else {
          ny = sy;
        }
        // Facing the light: bright; facing away: a dim reflection.
        const facing = nx * lx + ny * ly;
        const shade = 0.12 + 0.88 * (facing > 0 ? facing : 0.4 * -facing);
        const depth = Math.max(0, -distance);
        const crisp = (1 - clamp(depth / rim, 0, 1)) ** 2;
        const glow = (1 - clamp(depth / (rim * 5), 0, 1)) ** 2;
        const coverage = clamp(0.5 - distance, 0, 1);
        light[y * width + x] = clamp(specular * (crisp + 0.3 * glow) * shade * coverage, 0, 1);
      }
    }
  }
  const range = Math.max(1, max * clamp(optics.spread ?? 0.58, 0.3, 1));
  const data = new Uint8ClampedArray(width * height * 4);
  for (let y = 0; y < height; y += 1) {
    for (let x = 0; x < width; x += 1) {
      const i = (y * width + x) * 2;
      const p = (y * width + x) * 4;
      const edge = smooth(0, 3, Math.min(x, y, width - x - 1, height - y - 1));
      data[p] = Math.round(clamp((((raw[i] ?? 0) * edge) / range) * 0.5 + 0.5, 0, 1) * 255);
      data[p + 1] = Math.round(clamp((((raw[i + 1] ?? 0) * edge) / range) * 0.5 + 0.5, 0, 1) * 255);
      data[p + 2] = Math.round((light[y * width + x] ?? 0) * 255);
      data[p + 3] = 255;
    }
  }
  // Steepest inward fall of the encoded bend (±1 per map pixel). A bend larger than
  // 1/steepest folds the image: the rim would repeat content instead of magnifying it.
  const value = (index: number) => ((data[index] ?? 128) - 128) / 127;
  let steepest = 0;
  for (let y = 0; y < height; y += 1) {
    for (let x = 0; x < width; x += 1) {
      const p = (y * width + x) * 4;
      if (x + 1 < width) steepest = Math.max(steepest, value(p) - value(p + 4));
      if (y + 1 < height) steepest = Math.max(steepest, value(p + 1) - value(p + 1 + width * 4));
    }
  }
  return { data, scale: range * 2, unfolded: steepest > 0 ? 1 / steepest : range };
}

/** The three rendering engines; each converts SVG filter units differently. */
export type Engine = "blink" | "webkit" | "gecko";

/**
 * The engine behind a user agent, or null when unknown. Every iOS browser is
 * WebKit (CriOS, FxiOS), and so are WKWebView and WebKitGTK; WebView2 is Blink.
 */
export function engineOf(userAgent: string, brands: readonly string[] = []): Engine | null {
  if (brands.includes("Chromium")) return "blink";
  if (/\bFirefox\//.test(userAgent)) return "gecko";
  const chromium = /\b(?:Chrome|Chromium)\//.test(userAgent);
  if (/\bAppleWebKit\//.test(userAgent) && !chromium) return "webkit";
  return chromium ? "blink" : null;
}

/** A displacement `scale` in objectBoundingBox units, with WebKit's per-axis correction. */
export interface Bend {
  readonly scale: number;
  /** Factors that compress R and G towards neutral before displacing; null when not needed. */
  readonly squeeze: readonly [number, number] | null;
}

/**
 * Converts a maximum bend of `px` CSS pixels on a `width` × `height` element into
 * `feDisplacementMap` units under `primitiveUnits="objectBoundingBox"`. Measured:
 * Blink scales both axes by the width, WebKit x by the width and y by the height,
 * Gecko both by √((w² + h²)/2). Map values span ±0.5, so `scale` is twice the bend.
 */
export function bendScale(engine: Engine, px: number, width: number, height: number): Bend {
  if (![px, width, height].every(Number.isFinite) || width <= 0 || height <= 0 || px <= 0) {
    return { scale: 0, squeeze: null };
  }
  switch (engine) {
    case "blink":
      return { scale: (2 * px) / width, squeeze: null };
    case "gecko":
      return { scale: (2 * px) / Math.sqrt((width * width + height * height) / 2), squeeze: null };
    case "webkit": {
      const short = Math.min(width, height);
      return { scale: (2 * px) / short, squeeze: [short / width, short / height] };
    }
  }
}

/** `feColorMatrix` values that compress R by kx and G by ky around 0.5; B and A pass. */
export function squeezeMatrix([kx, ky]: readonly [number, number]): string {
  const offset = (k: number) => Number(((1 - k) / 2).toFixed(6));
  return [
    [kx, 0, 0, 0, offset(kx)],
    [0, ky, 0, 0, offset(ky)],
    [0, 0, 1, 0, 0],
    [0, 0, 0, 1, 0],
  ]
    .map((row) => row.map((v) => Number(v.toFixed(6))).join(" "))
    .join("  ");
}

export interface Lens {
  readonly url: string;
  /** The largest bend in CSS pixels that still magnifies without folding, measured. */
  readonly unfolded: number;
  readonly width: number;
  readonly height: number;
}
const maps = new Map<string, Lens>();

/** At most eight 2x maps keyed by shape; translation never invalidates them. */
export function lensMap(width: number, height: number, optics: LensOptics = {}): Lens | null {
  const w = Math.round(width * 2);
  const h = Math.round(height * 2);
  const key = [w, h, optics.bezel, optics.specular, optics.rim, optics.light, optics.spread].join(
    ":",
  );
  const cached = maps.get(key);
  if (cached) return cached;
  const pixels = lensPixels(w, h, h / 2, optics);
  if (!pixels) return null;
  const canvas = document.createElement("canvas");
  canvas.width = w;
  canvas.height = h;
  const context = canvas.getContext("2d");
  if (!context) return null;
  const image = context.createImageData(w, h);
  image.data.set(pixels.data);
  context.putImageData(image, 0, 0);
  const result = {
    url: canvas.toDataURL("image/png"),
    unfolded: pixels.unfolded / 2,
    width,
    height,
  };
  if (maps.size >= 8) maps.clear();
  maps.set(key, result);
  return result;
}
