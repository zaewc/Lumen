import { requireNativeModule } from "expo-modules-core";

/** The Rust core's native module (ADR-0007). */
interface LumenCore {
  /** The version of the Rust core the app loaded. */
  coreVersion(): string;
}

const native = requireNativeModule<LumenCore>("LumenCore");

/** The version of the Rust core the app loaded. */
export function coreVersion(): string {
  return native.coreVersion();
}
