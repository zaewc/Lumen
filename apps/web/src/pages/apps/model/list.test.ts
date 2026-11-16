import type { SoftwareItemView } from "@lumen/api-client";
import { describe, expect, it } from "vitest";
import { iconsShown, ordered, partial, title, tucked, visible } from "./list";

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
    facts: {
      mechanism: null,
      installed_by: null,
      needs_administrator: false,
      needs: null,
      needed_by: null,
    },
    ai_note: null,
    size: null,
    relations: [],
    relations_read: null,
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

  it("leaves room for icons only when the list has macOS apps", () => {
    expect(iconsShown(items)).toBe(false);
    expect(iconsShown([...items, row("dev.zed.Zed", { origin: "mac_app" })])).toBe(true);
    expect(iconsShown([])).toBe(false);
  });
});

describe("ordering by size", () => {
  it("puts measured apps first, largest first, and keeps the rest in order", () => {
    const sized = (id: string, size: SoftwareItemView["size"]) => row(id, { size });
    const list = [
      sized("a", null),
      sized("b", { kind: "complete", bytes: "10" }),
      sized("c", { kind: "not_scanned" }),
      sized("d", { kind: "at_least", bytes: "30" }),
      sized("e", { kind: "complete", bytes: "20" }),
    ];
    expect(ordered(list, "size").map((item) => item.id)).toEqual(["d", "e", "b", "a", "c"]);
    expect(ordered(list, "name")).toBe(list);
  });
});
