import { describe, expect, it } from "vitest";
import { outcomeText } from "./outcome";

describe("outcomeText", () => {
  it("says where every item is", () => {
    expect(outcomeText({ kind: "quarantined", operation: "op" })).toContain("restorable");
    expect(outcomeText({ kind: "skipped", reason: "now_review" })).toContain("review");
    expect(outcomeText({ kind: "failed", reason: "IdentityMismatch", left: "source" })).toContain(
      "untouched",
    );
  });
});
