import type { CandidateView } from "@lumen/api-client";
import { describe, expect, it } from "vitest";
import { memoryTransport } from "@/shared/api";
import { lastPlanWasItem, moveOffer, stillCurrent } from "./move";

async function sample(): Promise<CandidateView> {
  const items = await memoryTransport().assess();
  const review = items.find((item) => moveOffer(item, false) === "move");
  if (!review) throw new Error("a choosable review fixture is needed");
  return review;
}

describe("moving a review item the person chooses", () => {
  it("offers the button only for choosable review items, before a clean-up", async () => {
    const review = await sample();
    expect(moveOffer(review, false)).toBe("move");
    expect(moveOffer(review, true)).toBeNull();
    expect(moveOffer({ ...review, verdict: "keep" }, false)).toBeNull();
    expect(moveOffer({ ...review, verdict: "quarantine" }, false)).toBeNull();
  });

  it("says when a review item cannot be chosen", async () => {
    const review = await sample();
    const notRegenerable = {
      ...review,
      reasons: [{ rule: "classify.devkb-not-regenerable", text: "Cannot be made again" }],
    };
    const toolOnly = {
      ...review,
      reasons: [{ rule: "classify.devkb-tool-action", text: "Use the tool" }],
    };
    expect(moveOffer(notRegenerable, false)).toBe("unavailable");
    expect(moveOffer(toolOnly, false)).toBe("unavailable");
  });

  it("tells which action made the latest plan", () => {
    expect(lastPlanWasItem([])).toBe(false);
    expect(lastPlanWasItem([undefined])).toBe(false);
    expect(lastPlanWasItem([undefined, { id: "a", scan: "s" }])).toBe(true);
    expect(lastPlanWasItem([{ id: "a", scan: "s" }, undefined])).toBe(false);
  });

  it("drops a plan made for another scan", () => {
    expect(stillCurrent("scan-a", "scan-a")).toBe(true);
    expect(stillCurrent("scan-a", "scan-b")).toBe(false);
    expect(stillCurrent("scan-a", null)).toBe(false);
    expect(stillCurrent(null, null)).toBe(false);
  });
});
