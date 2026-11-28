import type { SoftwareItemView, SoftwareRelationView } from "@lumen/api-client";
import { describe, expect, it } from "vitest";
import { itemRelations, relationRecord } from "./relations";

function row(extra: Partial<SoftwareItemView> = {}): SoftwareItemView {
  return {
    key: "dpkg:mutt:amd64",
    origin: "dpkg",
    id: "mutt:amd64",
    name: null,
    version: null,
    vendor: null,
    location: null,
    protected: false,
    hidden: false,
    explicit: null,
    services: [],
    facts: {
      mechanism: "package_manager",
      installed_by: null,
      needs_administrator: true,
      needs: null,
      needed_by: { count: 0, complete: true },
    },
    relations: [],
    relations_read: true,
    ai_note: null,
    size: null,
    ...extra,
  };
}

function relation(
  kind: SoftwareRelationView["kind"],
  label: string,
  extra: Partial<SoftwareRelationView> = {},
): SoftwareRelationView {
  return {
    kind,
    other: null,
    label,
    method: "dpkg_status",
    direct: null,
    verified: false,
    ambiguous: false,
    ...extra,
  };
}

describe("an item's relations in its details", () => {
  it("names each record", () => {
    expect(relationRecord("cask_definition")).toBe("homebrew");
    expect(relationRecord("receipt_file_list")).toBe("receipt");
    expect(relationRecord("pacman_local_database")).toBe("pacman");
    expect(relationRecord("rpm_database")).toBe("rpm");
  });

  it("shows alternatives once, as one of several candidates", () => {
    const item = row({
      relations: [
        relation("needs", "libc6:amd64", { other: "dpkg:libc6:amd64", verified: true }),
        relation("needs", "postfix | exim4", { other: "dpkg:postfix", ambiguous: true }),
        relation("needs", "postfix | exim4", { other: "dpkg:exim4", ambiguous: true }),
        relation("needs", "gone"),
        relation("installed", "/usr/share/app"),
      ],
    });
    expect(itemRelations(item)).toEqual({
      needs: {
        kind: "entries",
        atLeast: false,
        entries: [
          { name: "libc6:amd64", record: "dpkg", certainty: "verified" },
          { name: "postfix | exim4", record: "dpkg", certainty: "candidate" },
          { name: "gone", record: "dpkg", certainty: "unconfirmed" },
        ],
      },
      neededBy: null,
    });
  });

  it("never says none where records were not read", () => {
    const unread = row({
      relations_read: false,
      facts: { ...row().facts, needed_by: { count: 0, complete: false } },
    });
    expect(itemRelations(unread)).toEqual({
      needs: { kind: "unread" },
      neededBy: { kind: "unread" },
    });
  });

  it("counts dependents as a lower bound unless every record was read", () => {
    const dependents = [relation("needed_by", "mutt:amd64", { ambiguous: true })];
    const partial = row({
      relations: dependents,
      facts: { ...row().facts, needed_by: { count: 0, complete: false } },
    });
    expect(itemRelations(partial).neededBy).toMatchObject({ kind: "entries", atLeast: true });
    expect(itemRelations(row({ relations: dependents })).neededBy).toMatchObject({
      kind: "entries",
      atLeast: false,
    });
  });

  it("shows nothing for an app or a package with no relations", () => {
    expect(itemRelations(row())).toEqual({ needs: null, neededBy: null });
    const app = row({
      origin: "mac_app",
      relations_read: null,
      facts: { ...row().facts, needed_by: null },
    });
    expect(itemRelations(app)).toEqual({ needs: null, neededBy: null });
  });
});
