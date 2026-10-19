/**
 * Frame statistics from `requestAnimationFrame` intervals on the JavaScript
 * thread (ADR-0007's spike). An interval longer than a frame means the JS
 * thread could not keep up; the UI thread's own frame rate is measured with
 * the platform's tools (see docs/research/11-mobile-spike.md).
 */
export interface FrameStats {
  /** Frames observed. */
  readonly frames: number;
  /** Median interval, in milliseconds. */
  readonly p50: number;
  /** 95th-percentile interval, in milliseconds. */
  readonly p95: number;
  /**
   * Share of intervals that missed at least one 60 Hz vsync (longer than 1.5
   * frames, 25 ms), 0..1. Normal vsync jitter around 16.7 ms is not counted.
   */
  readonly dropped: number;
}

/** Statistics over `intervals` (milliseconds). */
export function frameStats(intervals: readonly number[]): FrameStats {
  if (intervals.length === 0) {
    return { frames: 0, p50: 0, p95: 0, dropped: 0 };
  }
  const sorted = [...intervals].sort((a, b) => a - b);
  const at = (q: number) => sorted[Math.min(sorted.length - 1, Math.floor(q * sorted.length))] ?? 0;
  const dropped = intervals.filter((ms) => ms > 1.5 * (1000 / 60)).length / intervals.length;
  return { frames: intervals.length, p50: at(0.5), p95: at(0.95), dropped };
}
