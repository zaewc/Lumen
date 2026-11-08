import type { ToolKindView, ToolStorageItemView, ToolStorageView } from "@lumen/api-client";

/** Product names stay as their makers write them, in every language. */
export const TOOL_NAMES: { readonly [K in ToolKindView]: string } = {
  xcode: "Xcode",
  docker: "Docker",
  homebrew: "Homebrew",
  npm: "npm",
  pnpm: "pnpm",
};

/**
 * The version number a tool printed ("Homebrew 7.0.12" and "Docker version
 * 28.2.2, build e6534b4" → "7.0.12", "28.2.2"); anything else as printed.
 */
export function versionText(view: ToolStorageView): string | null {
  if (view.version === null) return null;
  return /\d+(?:\.\d+)+/.exec(view.version)?.[0] ?? view.version;
}

/**
 * An item's size as the tool gave it: an exact byte count, Docker's own
 * wording, or nothing. A missing size is unknown, never zero.
 */
export type ItemSize =
  | { readonly kind: "bytes"; readonly value: string }
  | { readonly kind: "reported"; readonly text: string }
  | { readonly kind: "unknown" };

export function itemSize(item: ToolStorageItemView): ItemSize {
  if (item.bytes !== null) return { kind: "bytes", value: item.bytes };
  if (item.reported_size !== null) return { kind: "reported", text: item.reported_size };
  return { kind: "unknown" };
}
