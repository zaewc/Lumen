import type { CandidateView } from "@lumen/api-client";
import { describe, expect, it } from "vitest";
import { memoryTransport } from "@/shared/api";
import { latestPlan, moveOffer, type PlanState, stillCurrent } from "./move";

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

  it("tells which action made the latest plan and whether it failed", () => {
    const toolbar: PlanState = { variables: undefined, status: "error", error: "x" };
    const item: PlanState = { variables: { id: "a", scan: "s" }, status: "success", error: null };
    expect(latestPlan([])).toBeNull();
    expect(latestPlan([toolbar])).toEqual({ item: null, failed: true, error: "x" });
    expect(latestPlan([toolbar, item])).toEqual({ item: "a", failed: false, error: null });
    expect(latestPlan([item, { ...toolbar, status: "pending", error: null }])).toEqual({
      item: null,
      failed: false,
      error: null,
    });
  });

  it("drops a plan made for another scan", () => {
    expect(stillCurrent("scan-a", "scan-a")).toBe(true);
    expect(stillCurrent("scan-a", "scan-b")).toBe(false);
    expect(stillCurrent("scan-a", null)).toBe(false);
    expect(stillCurrent(null, null)).toBe(false);
  });
});
