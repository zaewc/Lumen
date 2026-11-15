import type { UnreadFoldersView, UnreadReasonView } from "@lumen/api-client";
import { describe, expect, it } from "vitest";
import { en } from "@/shared/i18n";
import { REASON_ORDER, unlistedCount, unreadGroups } from "./unread";

function view(total: string, folders: [string, UnreadReasonView][]): UnreadFoldersView {
  return {
    scan: "s",
    total,
    folders: folders.map(([path, reason]) => ({ path, reason })),
  };
}

describe("unread folders", () => {
  it("groups by reason, actionable reasons first, keeping path order", () => {
    const groups = unreadGroups(
      view("4", [
        ["/a", "permissions"],
        ["/b", "privacy"],
        ["/c", "not_entered"],
        ["/d", "privacy"],
      ]),
    );
    expect(groups).toEqual([
      { reason: "privacy", paths: ["/b", "/d"] },
      { reason: "permissions", paths: ["/a"] },
      { reason: "not_entered", paths: ["/c"] },
    ]);
  });

  it("orders every reason exactly once", () => {
    expect(new Set(REASON_ORDER).size).toBe(REASON_ORDER.length);
    // The catalog is checked against every reason the schema has.
    expect([...REASON_ORDER].sort()).toEqual(Object.keys(en.details.unread.reasons).sort());
  });

  it("counts the folders beyond those listed, never below zero", () => {
    expect(unlistedCount(view("12", [["/a", "privacy"]]))).toBe("11");
    expect(unlistedCount(view("1", [["/a", "privacy"]]))).toBe("0");
    expect(unlistedCount(view("not a number", []))).toBe("0");
    expect(unlistedCount(view("18446744073709551615", []))).toBe("18446744073709551615");
  });
});
