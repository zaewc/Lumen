import { invoke } from "@tauri-apps/api/core";
import { z } from "zod";
import { hostError } from "./host-error";

const zItem = z.object({ path: z.string(), bytes: z.string().nullable() });
const zListed = z.object({ items: z.array(zItem), more: z.number().int().nonnegative() });

/**
 * What the desktop host asks the person to confirm in its own window (FS-1).
 * The host builds it from what it stored; this page only shows it.
 */
export const zHostAsk = z.discriminatedUnion("kind", [
  z.object({
    kind: z.literal("move"),
    count: z.number().int().nonnegative(),
    total_bytes: z.string(),
    proposed: zListed,
    chosen: z.array(zItem),
  }),
  z.object({ kind: z.literal("uninstall"), items: z.array(zItem) }),
  z.object({ kind: z.literal("finalize"), item: zItem, chosen: z.boolean() }),
  z.object({ kind: z.literal("tool"), tool: z.string(), command: z.string(), targets: zListed }),
]);

export type HostAsk = z.infer<typeof zHostAsk>;
export type HostAskItem = z.infer<typeof zItem>;

/**
 * The confirmation window's two calls; only that window may make them (its
 * own capability). The main window never uses this.
 */
export const confirmHost = {
  /** The question the host asks now. */
  request: async (): Promise<HostAsk> => {
    try {
      return zHostAsk.parse(await invoke<unknown>("confirm_request"));
    } catch (rejection) {
      throw hostError(rejection);
    }
  },
  /** The person's answer; the host closes the window. */
  answer: async (yes: boolean): Promise<void> => {
    try {
      await invoke("confirm_answer", { yes });
    } catch (rejection) {
      throw hostError(rejection);
    }
  },
};
