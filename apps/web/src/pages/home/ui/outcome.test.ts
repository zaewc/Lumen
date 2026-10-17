import { describe, expect, it } from "vitest";
import { daysHeld, finalizeText, restoreText } from "./outcome";

describe("held-item outcomes", () => {
  it("explain conflicts and in-use refusals instead of hiding them", () => {
    expect(restoreText({ kind: "not_restored", reason: "name_taken" })).toContain(
      "stays in quarantine",
    );
    expect(restoreText({ kind: "restored", parent_replaced: true, verified: false })).toContain(
      "check it",
    );
    expect(finalizeText({ kind: "maybe_in_use" })).toContain("nothing was removed");
    expect(finalizeText({ kind: "failed", removed: "partly" })).toContain("try again");
  });

  it("counts whole days held", () => {
    const now = new Date("2026-10-31T12:00:00Z");
    expect(daysHeld("2026-10-01T09:00:00Z", now)).toBe(30);
    expect(daysHeld("2026-11-01T00:00:00Z", now)).toBe(0);
  });
});
