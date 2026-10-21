import type { RootView } from "@lumen/api-client";
import { isMac } from "@/shared/lib";

/** What the user should know about folders the scan could not read. */
export interface Coverage {
  /** Some root was not read completely. */
  readonly incomplete: boolean;
  /** Reading was denied on a Mac: most likely privacy protection (TCC). */
  readonly macPrivacy: boolean;
}

function positive(decimal: string): boolean {
  return /^\d+$/.test(decimal) && BigInt(decimal) > 0n;
}

/** Whether any root is incomplete and whether Full Disk Access may help. */
export function coverageOf(roots: readonly RootView[], userAgent: string): Coverage {
  const incomplete = roots.some((root) => !root.coverage_complete);
  const denied = roots.some((root) => positive(root.denied));
  return { incomplete, macPrivacy: incomplete && denied && isMac(userAgent) };
}
