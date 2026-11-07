import type { SoftwareItemView } from "@lumen/api-client";
import { describe, expect, it } from "vitest";
import { partial, title, tucked, visible } from "./list";

function row(id: string, extra: Partial<SoftwareItemView> = {}): SoftwareItemView {
  return {
    key: `homebrew:${id}`,
    origin: "homebrew",
    id,
    name: null,
    version: "1.0",
    vendor: null,
    location: null,
    protected: false,
    hidden: false,
    explicit: null,
    services: [],
    ...extra,
  };
}

const items = [
  row("act", { explicit: true }),
  row("openssl@3", { explicit: false }),
  row("com.example.Editor", { name: "Old Editor", vendor: "Example Corp" }),
  row("{1111}", { name: "Runtime", hidden: true }),
];

describe("software list", () => {
  it("leaves out dependencies and hidden entries unless all are asked for", () => {
    expect(visible(items, { query: "", all: false }).map((i) => i.id)).toEqual([
      "act",
      "com.example.Editor",
    ]);
    expect(visible(items, { query: "", all: true })).toHaveLength(4);
    expect(tucked(items)).toBe(2);
  });

  it("searches name, identifier, vendor and version by every word", () => {
    expect(visible(items, { query: "example old", all: false }).map(title)).toEqual(["Old Editor"]);
    expect(visible(items, { query: "OPENSSL", all: true }).map(title)).toEqual(["openssl@3"]);
    expect(visible(items, { query: "nothing", all: true })).toEqual([]);
  });

  it("names partial inventories only", () => {
    expect(
      partial([
        { source: "macos.homebrew", state: "partial", count: 3 },
        { source: "macos.applications", state: "complete", count: 9 },
        { source: "linux.snap", state: "unavailable", count: 0 },
      ]),
    ).toEqual(["macos.homebrew"]);
  });
});
