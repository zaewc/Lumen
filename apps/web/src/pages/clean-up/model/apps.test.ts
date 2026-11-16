import type { SoftwareItemView } from "@lumen/api-client";
import { describe, expect, it } from "vitest";
import { APPS_SHOWN, appsSection } from "./apps";

function app(id: string, size: SoftwareItemView["size"], extra: Partial<SoftwareItemView> = {}) {
  const item: SoftwareItemView = {
    key: `mac_app:${id}`,
    origin: "mac_app",
    id,
    name: id,
    version: null,
    vendor: null,
    location: `/Applications/${id}.app`,
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
  return item;
}

const complete = (bytes: number): SoftwareItemView["size"] => ({
  kind: "complete",
  bytes: String(bytes),
});

describe("Explore's Apps section", () => {
  it("lists apps of at least 100 MB, largest first, a lower bound by its value", () => {
    const section = appsSection([
      app("Small", complete(99_999_999)),
      app("Big", complete(5_000_000_000)),
      app("Part", { kind: "at_least", bytes: "3100000000" }),
      app("Mid", complete(1_200_000_000)),
    ]);
    expect(section?.rows.map((row) => [row.item.id, row.atLeast])).toEqual([
      ["Big", false],
      ["Part", true],
      ["Mid", false],
    ]);
    expect(section?.bytes).toBe("9300000000");
    expect(section?.atLeast, "a lower bound in the list makes the total one").toBe(true);
    expect(appsSection([app("Big", complete(5_000_000_000))])?.atLeast).toBe(false);
  });

  it("shows at most ten and counts the apps outside the scan", () => {
    const many = Array.from({ length: APPS_SHOWN + 3 }, (_, n) =>
      app(`A${n}`, complete(200_000_000 + n)),
    );
    const section = appsSection([
      ...many,
      app("Away", { kind: "not_scanned" }),
      app("Gone", { kind: "not_scanned" }),
    ]);
    expect(section?.rows).toHaveLength(APPS_SHOWN);
    expect(section?.rows[0]?.item.id).toBe(`A${APPS_SHOWN + 2}`);
    expect(section?.outside).toBe(2);
  });

  it("leaves out system apps, packages, and the section when no app was measured", () => {
    const brew: SoftwareItemView = {
      ...app("act", complete(900_000_000)),
      origin: "homebrew",
      key: "homebrew:act",
    };
    expect(
      appsSection([app("Calculator", complete(900_000_000), { protected: true }), brew]),
    ).toBeNull();
    expect(appsSection([app("Away", { kind: "not_scanned" })])).toBeNull();
    expect(appsSection([app("Tiny", complete(10))])?.rows).toEqual([]);
  });
});
