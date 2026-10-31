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
const clamp = (v: number, low: number, high: number) => Math.max(low, Math.min(high, v));
const smooth = (low: number, high: number, v: number) => {
  const t = clamp((v - low) / (high - low || 1), 0, 1);
  return t * t * (3 - 2 * t);
};

/** Pure, bounded inverse mapping: identity centre, curved thin bezel, neutral rim. */
export function lensPixels(width: number, height: number, radius: number) {
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
  const bezel = clamp(Math.min(width, height) * 0.5 * 0.62, 10, 220);
  const raw = new Float32Array(width * height * 2);
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
    }
  }
  const range = Math.max(1, max * 0.58);
  const data = new Uint8ClampedArray(width * height * 4);
  for (let y = 0; y < height; y += 1) {
    for (let x = 0; x < width; x += 1) {
      const i = (y * width + x) * 2;
      const p = (y * width + x) * 4;
      const edge = smooth(0, 3, Math.min(x, y, width - x - 1, height - y - 1));
      data[p] = Math.round(clamp((((raw[i] ?? 0) * edge) / range) * 0.5 + 0.5, 0, 1) * 255);
      data[p + 1] = Math.round(clamp((((raw[i + 1] ?? 0) * edge) / range) * 0.5 + 0.5, 0, 1) * 255);
      data[p + 3] = 255;
    }
  }
  return { data, scale: range * 2 };
}

interface Lens {
  readonly url: string;
  readonly scale: number;
  readonly width: number;
  readonly height: number;
}
const maps = new Map<string, Lens>();

/** At most eight 2x maps; pointer motion/translation never invalidates them. */
export function lensMap(width: number, height: number): Lens | null {
  const w = Math.round(width * 2);
  const h = Math.round(height * 2);
  const key = `${w}:${h}`;
  const cached = maps.get(key);
  if (cached) return cached;
  const pixels = lensPixels(w, h, h / 2);
  if (!pixels) return null;
  const canvas = document.createElement("canvas");
  canvas.width = w;
  canvas.height = h;
  const context = canvas.getContext("2d");
  if (!context) return null;
  const image = context.createImageData(w, h);
  image.data.set(pixels.data);
  context.putImageData(image, 0, 0);
  const result = { url: canvas.toDataURL("image/png"), scale: pixels.scale / 2, width, height };
  if (maps.size >= 8) maps.clear();
  maps.set(key, result);
  return result;
}
