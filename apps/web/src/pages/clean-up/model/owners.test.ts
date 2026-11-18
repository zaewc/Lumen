import type { CandidateView, SoftwareItemView } from "@lumen/api-client";
import { describe, expect, it } from "vitest";
import { ownersOf } from "./owners";

function candidate(
  path: string,
  verdict: CandidateView["verdict"],
  bytes: string,
  group: CandidateView["group"],
  exact = true,
): CandidateView {
  return {
    id: path,
    path,
    verdict,
    reclaimable_bytes: bytes,
    reclaimable_exact: exact,
    reasons: [],
    tool_command: null,
    group,
    ai_note: null,
    free_when_closed: null,
    times: { modified: { state: "not_read" }, accessed: { state: "not_read" }, folder: true },
    facts: [],
    relations: { edges: [], omitted: "0" },
  };
}

function app(
  id: string,
  name: string,
  size: SoftwareItemView["size"],
  extra: Partial<SoftwareItemView> = {},
): SoftwareItemView {
  return {
    key: `mac_app:${id}`,
    origin: "mac_app",
    id,
    name,
    version: null,
    vendor: null,
    location: `/Applications/${name}.app`,
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
    size,
    relations: [],
    relations_read: null,
    ...extra,
  };
}

const xcode = app("com.apple.dt.Xcode", "Xcode", { kind: "complete", bytes: "14800" });
const slack = app("com.tinyspeck.slackmacgap", "Slack", { kind: "at_least", bytes: "500" });
const steam = app("com.valvesoftware.steam", "Steam", { kind: "not_scanned" });
const calculator = app(
  "com.apple.calculator",
  "Calculator",
  { kind: "complete", bytes: "9" },
  {
    protected: true,
  },
);

describe("owners", () => {
  it("puts a tool's items and an app's caches under the installed app", () => {
    const owners = ownersOf(
      [
        candidate("/dd", "quarantine", "30100", { kind: "developer", source: "Xcode" }),
        candidate("/sim", "review", "8200", { kind: "developer", source: "xcode" }),
        candidate("/slack-cache", "quarantine", "1100", {
          kind: "app_cache",
          source: "com.tinyspeck.slackmacgap",
        }),
      ],
      [xcode, slack],
    );
    expect(owners.map((o) => [o.key, o.kind, o.name])).toEqual([
      ["app:com.apple.dt.Xcode", "app", "Xcode"],
      ["app:com.tinyspeck.slackmacgap", "app", "Slack"],
    ]);
    const [first, second] = owners;
    expect(first?.parts.map((p) => (p.kind === "app" ? "app" : p.item.path))).toEqual([
      "app",
      "/dd",
      "/sim",
    ]);
    expect([first?.bytes, first?.proposedBytes, first?.atLeast]).toEqual(["53100", "30100", false]);
    expect([second?.bytes, second?.atLeast], "the app's size is a lower bound").toEqual([
      "1600",
      true,
    ]);
  });

  it("groups tools, unknown owners, removed apps and kinds apart, largest first", () => {
    const owners = ownersOf(
      [
        candidate("/pnpm", "review", "12100", { kind: "developer", source: "pnpm" }),
        candidate("/cache", "quarantine", "300", { kind: "app_cache", source: "com.gone.app" }),
        candidate("/left-a", "review", "1400", {
          kind: "removed_app",
          source: "com.spotify.client",
        }),
        candidate("/left-b", "review", "600", { kind: "removed_app", source: "com.adobe.xd" }),
        candidate("/logs", "quarantine", "800", { kind: "other", source: null }, false),
      ],
      [],
    );
    expect(owners.map((o) => [o.key, o.kind, o.name, o.group])).toEqual([
      ["tool:pnpm", "tool", "pnpm", null],
      ["removed", "removed", null, null],
      ["kind:other", "kind", null, "other"],
      ["source:com.gone.app", "source", "com.gone.app", null],
    ]);
    expect(owners[1]?.parts).toHaveLength(2);
    expect(owners[2]?.atLeast, "an inexact item makes its total a lower bound").toBe(true);
  });

  it("lists every movable app, never the system's, and never sizes an unmeasured one", () => {
    const owners = ownersOf([], [xcode, steam, calculator]);
    expect(owners.map((o) => o.key)).toEqual([
      "app:com.apple.dt.Xcode",
      "app:com.valvesoftware.steam",
    ]);
    const unmeasured = owners[1];
    expect([unmeasured?.bytes, unmeasured?.atLeast]).toEqual([null, true]);
    expect(unmeasured?.parts[0]).toMatchObject({ kind: "app", bytes: null });
  });

  it("keeps a system app's items under it, without an app part", () => {
    const owners = ownersOf(
      [
        candidate("/calc-cache", "quarantine", "50", {
          kind: "app_cache",
          source: "com.apple.calculator",
        }),
      ],
      [calculator],
    );
    expect(owners).toHaveLength(1);
    expect(owners[0]?.parts.map((p) => p.kind)).toEqual(["item"]);
  });

  it("puts every candidate under exactly one owner", () => {
    const items = [
      candidate("/a", "keep", "1", { kind: "app_data", source: "com.apple.dt.Xcode" }),
      candidate("/b", "review", "2", { kind: "developer", source: null }),
      candidate("/c", "quarantine", "3", { kind: "removed_app", source: null }),
    ];
    const owned = ownersOf(items, [xcode]).flatMap((o) =>
      o.parts.flatMap((p) => (p.kind === "item" ? [p.item.path] : [])),
    );
    expect(owned.toSorted()).toEqual(["/a", "/b", "/c"]);
  });
});
