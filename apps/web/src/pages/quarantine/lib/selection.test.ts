import type { HeldItemView } from "@lumen/api-client";
import { describe, expect, it } from "vitest";
import { en, ko } from "@/shared/i18n";
import {
  actionable,
  bulkResult,
  bulkText,
  chosen,
  selectionState,
  stillHeld,
  toggled,
} from "./selection";

function item(operation: string, state: HeldItemView["state"] = "retained"): HeldItemView {
  return {
    operation,
    original: `/Users/ana/${operation}`,
    reclaim_bytes: "1000",
    since: "2026-10-20T00:00:00Z",
    state,
  };
}

describe("selecting held items", () => {
  const items = [item("a"), item("b"), item("c", "held_after_failure")];

  it("tells all, some and none apart, and toggles one item", () => {
    expect(selectionState(items, new Set())).toBe("none");
    expect(selectionState(items, new Set(["a"]))).toBe("some");
    expect(selectionState(items, new Set(["a", "b", "c"]))).toBe("all");
    expect([...toggled(new Set(["a"]), "b")]).toEqual(["a", "b"]);
    expect([...toggled(new Set(["a", "b"]), "a")]).toEqual(["b"]);
  });

  it("drops items that left the list and keeps list order", () => {
    expect([...stillHeld(items.slice(1), new Set(["a", "b"]))]).toEqual(["b"]);
    expect(chosen(items, new Set(["c", "a"])).map((i) => i.operation)).toEqual(["a", "c"]);
  });

  it("restores any item but removes for good only retained ones", () => {
    expect(actionable("restore", items).act).toHaveLength(3);
    const remove = actionable("finalize", items);
    expect(remove.act.map((i) => i.operation)).toEqual(["a", "b"]);
    expect(remove.skipped).toBe(1);
  });

  it("sums up what happened, in each language", () => {
    const restored = { kind: "restored", parent_replaced: false, verified: true } as const;
    const all = bulkResult("restore", [
      { action: "restore", outcome: restored },
      { action: "restore", outcome: restored },
    ]);
    expect(all).toEqual({ action: "restore", total: 2, well: 2 });
    expect(bulkText(en, all)).toBe("Restored 2 items.");
    expect(bulkText(ko, all)).toBe("항목 2개를 복원했어요.");
    const partly = bulkResult("finalize", [
      { action: "finalize", outcome: { kind: "finalized" } },
      { action: "finalize", outcome: { kind: "maybe_in_use" } },
      { action: "finalize", outcome: { kind: "finalized" } },
    ]);
    expect(bulkText(ko, partly)).toBe(
      "항목 3개 중 2개를 영구 삭제했어요. 나머지는 삭제하지 않았어요.",
    );
    expect(bulkText(en, partly)).toBe("Removed 2 of 3 items for good. Lumen removed nothing else.");
  });
});
