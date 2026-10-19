import { requireNativeModule } from "expo-modules-core";

/** Near-duplicate groups among the newest photos (ADR-0007's spike). */
export interface Analysis {
  /** Photos analyzed. */
  readonly analyzed: number;
  /** Groups of two or more near-duplicates, as the photo IDs the grid uses. */
  readonly groups: readonly (readonly string[])[];
  /** Time spent on thumbnails and feature prints, in milliseconds. */
  readonly printMs: number;
  /** Time spent grouping in Rust, in milliseconds. */
  readonly groupMs: number;
  /** Whether the person cancelled the background task. */
  readonly cancelled: boolean;
}

/** The Rust core's native module (ADR-0007). */
interface LumenCore {
  /** The version of the Rust core the app loaded. */
  coreVersion(): string;
  analyze(limit: number, threshold: number): Promise<Analysis>;
}

const native = requireNativeModule<LumenCore>("LumenCore");

/** The version of the Rust core the app loaded. */
export function coreVersion(): string {
  return native.coreVersion();
}

/**
 * Finds near-duplicates among the newest `limit` photos: feature prints are
 * computed natively and grouped in Rust; only the groups come back.
 */
export function analyze(limit: number, threshold: number): Promise<Analysis> {
  return native.analyze(limit, threshold);
}
