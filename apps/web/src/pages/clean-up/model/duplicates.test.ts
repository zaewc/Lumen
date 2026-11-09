import type { DuplicatesView } from "@lumen/api-client";
import { describe, expect, it } from "vitest";
import { compareNotes } from "./duplicates";

const report: DuplicatesView = {
  folders: ["/Users/a/Downloads"],
  groups: [],
  hard_links: [],
  skipped: "0",
  protected: "0",
  incomplete: false,
};

describe("compareNotes", () => {
  it("says nothing when every file was compared", () => {
    expect(compareNotes(report)).toEqual([]);
  });

  it("names what was not compared, and an incomplete listing", () => {
    expect(
      compareNotes({
        ...report,
        hard_links: [["/a", "/b"]],
        skipped: "3",
        protected: "12",
        incomplete: true,
      }),
    ).toEqual([
      { kind: "hard_links", count: 1 },
      { kind: "skipped", count: 3 },
      { kind: "protected", count: 12 },
      { kind: "incomplete" },
    ]);
  });
});
