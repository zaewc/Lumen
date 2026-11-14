import { describe, expect, it } from "vitest";
import { memoryTransport } from "./memory";
import { canChooseReview, chosenPlan } from "./review-choice";

describe("explicit review choices", () => {
  it("keeps review choices out until selected and confirms their exact plan once", async () => {
    const transport = memoryTransport();
    const items = await transport.assess();
    const item = items.find(canChooseReview);
    if (!item) throw new Error("review fixture missing");
    const before = await transport.held();
    const automatic = await transport.plan();
    expect(automatic.items.every((c) => c.verdict === "quarantine")).toBe(true);
    expect(() => chosenPlan(automatic, [item.id])).toThrow();
    const selected = await transport.plan([item.id]);
    expect(selected.items.map((c) => c.id)).toEqual([item.id]);
    expect(() =>
      chosenPlan({ ...selected, items: [...selected.items, ...automatic.items] }, [item.id]),
    ).toThrow();
    expect(await transport.held()).toEqual(before);
    await expect(
      transport.execute({ plan: automatic.plan, confirmation: automatic.hash }),
    ).rejects.toThrow();
    const request = { plan: selected.plan, confirmation: selected.hash };
    await transport.execute(request);
    expect((await transport.held()).find((c) => c.original === item.path)?.chosen_by).toBe(
      "person",
    );
    await expect(transport.execute(request)).rejects.toThrow();
  });

  it("refuses unknown, kept, tool-only and non-regenerable choices", async () => {
    const transport = memoryTransport();
    const items = await transport.assess();
    for (const item of items.filter((c) => !canChooseReview(c))) {
      await expect(transport.plan([item.id])).rejects.toThrow();
    }
    await expect(transport.plan(["unknown-id"])).rejects.toThrow();
    const sample = items[0];
    if (!sample) throw new Error("fixture missing");
    expect(
      canChooseReview({
        ...sample,
        verdict: "review",
        tool_command: null,
        reasons: [{ rule: "classify.devkb-not-regenerable", text: "Not regenerable" }],
      }),
    ).toBe(false);
  });
});
