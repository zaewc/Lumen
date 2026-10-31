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
    chosen_by: "policy",
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
    expect(all).toEqual({ action: "restore", total: 2, well: 2, stopped: null });
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

  it("says what was already removed when an error stopped the run", () => {
    const removed = { action: "finalize", outcome: { kind: "finalized" } } as const;
    const error = { message: "another command is still running" };
    // Items 1-3 removed, item 4 failed, items 5-10 never tried.
    const stopped = bulkResult("finalize", [removed, removed, removed], { error, untried: 6 });
    expect(stopped).toEqual({
      action: "finalize",
      total: 10,
      well: 3,
      stopped: { error, untried: 6 },
    });
    expect(bulkText(en, stopped)).toBe(
      "Removed 3 of 10 items for good, then an error stopped Lumen. It didn't try the other 6. What's still in the list stays in Quarantine, and you can restore it.",
    );
    expect(bulkText(ko, stopped)).toBe(
      "항목 10개 중 3개를 영구 삭제하고, 오류가 생겨서 멈췄어요. 나머지 6개는 삭제하지 않았어요. 목록에 남은 항목은 격리함에 그대로 있어서 복원할 수 있어요.",
    );
    const last = bulkResult("finalize", [removed], { error, untried: 0 });
    expect(last.total).toBe(2);
    expect(bulkText(en, last)).toBe(
      "Removed 1 of 2 items for good, then an error stopped Lumen. What's still in the list stays in Quarantine, and you can restore it.",
    );
    const restored = { kind: "restored", parent_replaced: false, verified: true } as const;
    const restoring = bulkResult("restore", [{ action: "restore", outcome: restored }], {
      error,
      untried: 2,
    });
    expect(bulkText(ko, restoring)).toBe(
      "항목 4개 중 1개를 복원하고, 오류가 생겨서 멈췄어요. 나머지 2개는 복원하지 않았어요. 목록에 남은 항목은 격리함에 그대로 있어요.",
    );
  });
});
