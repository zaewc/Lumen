import type { ToolStorageItemView, ToolStorageView } from "@lumen/api-client";
import { describe, expect, it } from "vitest";
import { itemSize, versionText } from "./tools";

const item = (overrides: Partial<ToolStorageItemView>): ToolStorageItemView => ({
  name: "Images",
  detail: null,
  bytes: null,
  reported_size: null,
  reported_reclaimable: null,
  ...overrides,
});

const view = (overrides: Partial<ToolStorageView>): ToolStorageView => ({
  tool: "homebrew",
  state: { kind: "found" },
  version: null,
  scope: null,
  items: [],
  command: null,
  shared_store: false,
  ...overrides,
});

describe("itemSize", () => {
  it("prefers an exact byte count, then the tool's own wording", () => {
    expect(itemSize(item({ bytes: "8724152320", reported_size: "8GB" }))).toEqual({
      kind: "bytes",
      value: "8724152320",
    });
    expect(itemSize(item({ reported_size: "5.2GB" }))).toEqual({ kind: "reported", text: "5.2GB" });
  });

  it("keeps a missing size unknown rather than zero", () => {
    expect(itemSize(item({}))).toEqual({ kind: "unknown" });
  });
});

describe("versionText", () => {
  it("keeps the version number and nothing around it", () => {
    expect(versionText(view({ version: "Homebrew 7.0.12" }))).toBe("7.0.12");
    expect(versionText(view({ tool: "npm", version: "11.4.1" }))).toBe("11.4.1");
    expect(
      versionText(view({ tool: "docker", version: "Docker version 28.2.2, build e6534b4" })),
    ).toBe("28.2.2");
    expect(versionText(view({ version: "nightly" }))).toBe("nightly");
    expect(versionText(view({}))).toBeNull();
  });
});
