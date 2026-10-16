import { describe, expect, it } from "vitest";
import { en, ko } from "@/shared/i18n";
import { daysHeld, doneText, doneWell, finalizeText, restoreText } from "./outcome";

describe("held-item outcomes", () => {
  it("explain conflicts and in-use refusals instead of hiding them", () => {
    expect(restoreText(en, { kind: "not_restored", reason: "name_taken" })).toContain(
      "stays in Quarantine",
    );
    expect(restoreText(en, { kind: "restored", parent_replaced: true, verified: false })).toContain(
      "check it",
    );
    expect(finalizeText(en, { kind: "maybe_in_use" })).toContain("Lumen removed nothing");
    expect(finalizeText(en, { kind: "failed", removed: "partly" })).toContain("Try again");
  });

  it("speak Korean when given the Korean catalog", () => {
    expect(restoreText(ko, { kind: "not_restored", reason: "name_taken" })).toBe(
      "복원하지 못했어요. 항목은 격리함에 그대로 있어요. 원래 위치에 같은 이름의 항목이 생겼어요. 그 항목을 다른 곳으로 옮긴 뒤 다시 시도해 주세요.",
    );
    expect(restoreText(ko, { kind: "restored", parent_replaced: false, verified: true })).toBe(
      "원래 위치로 복원했어요.",
    );
    expect(finalizeText(ko, { kind: "finalized" })).toBe("영구 삭제했어요.");
    expect(finalizeText(ko, { kind: "failed", removed: "nothing" })).toBe(
      "완전히 삭제하지 못했어요. 아무것도 삭제하지 않았어요.",
    );
  });

  it("say what the last action did, and whether it needs a look", () => {
    const restored = {
      action: "restore",
      outcome: { kind: "restored", parent_replaced: false, verified: true },
    } as const;
    expect(doneText(en, restored)).toBe("Restored to its original place.");
    expect(doneWell(restored)).toBe(true);
    expect(
      doneWell({
        action: "restore",
        outcome: { kind: "restored", parent_replaced: false, verified: false },
      }),
    ).toBe(false);
    expect(doneWell({ action: "finalize", outcome: { kind: "finalized" } })).toBe(true);
    expect(doneWell({ action: "finalize", outcome: { kind: "maybe_in_use" } })).toBe(false);
    expect(doneText(ko, { action: "finalize", outcome: { kind: "not_found" } })).toBe(
      "격리함에 없어요. 아무것도 삭제하지 않았어요.",
    );
  });

  it("counts whole days held", () => {
    const now = new Date("2026-10-31T12:00:00Z");
    expect(daysHeld("2026-10-01T09:00:00Z", now)).toBe(30);
    expect(daysHeld("2026-11-01T00:00:00Z", now)).toBe(0);
  });
});
