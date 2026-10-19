import { describe, expect, it } from "vitest";
import { en, ko } from "@/shared/i18n";
import { daysHeld, finalizeText, restoreText } from "./outcome";

describe("held-item outcomes", () => {
  it("explain conflicts and in-use refusals instead of hiding them", () => {
    expect(restoreText(en, { kind: "not_restored", reason: "name_taken" })).toContain(
      "stays in quarantine",
    );
    expect(restoreText(en, { kind: "restored", parent_replaced: true, verified: false })).toContain(
      "check it",
    );
    expect(finalizeText(en, { kind: "maybe_in_use" })).toContain("nothing was removed");
    expect(finalizeText(en, { kind: "failed", removed: "partly" })).toContain("try again");
  });

  it("speak Korean when given the Korean catalog", () => {
    expect(restoreText(ko, { kind: "not_restored", reason: "name_taken" })).toBe(
      "복원하지 못했습니다: 원래 위치에 같은 이름의 항목이 생겼습니다. 그 항목을 다른 곳으로 옮긴 뒤 다시 시도하십시오. 격리 상태로 유지됩니다.",
    );
    expect(restoreText(ko, { kind: "restored", parent_replaced: false, verified: true })).toBe(
      "원래 위치로 복원했습니다.",
    );
    expect(finalizeText(ko, { kind: "finalized" })).toBe("영구 삭제했습니다.");
    expect(finalizeText(ko, { kind: "failed", removed: "nothing" })).toBe(
      "완전히 삭제하지 못했습니다: 아무것도 삭제되지 않았습니다.",
    );
  });

  it("counts whole days held", () => {
    const now = new Date("2026-10-31T12:00:00Z");
    expect(daysHeld("2026-10-01T09:00:00Z", now)).toBe(30);
    expect(daysHeld("2026-11-01T00:00:00Z", now)).toBe(0);
  });
});
