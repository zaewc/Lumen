import { describe, expect, it } from "vitest";
import { en, ko } from "@/shared/i18n";
import { outcomeText } from "./outcome";

describe("outcomeText", () => {
  it("says where every item is", () => {
    expect(outcomeText(en, { kind: "quarantined", operation: "op" })).toContain("restorable");
    expect(outcomeText(en, { kind: "skipped", reason: "now_review" })).toContain("review");
    expect(
      outcomeText(en, { kind: "failed", reason: "IdentityMismatch", left: "source" }),
    ).toContain("untouched");
  });

  it("says it in Korean, keeping the core's failure code", () => {
    expect(outcomeText(ko, { kind: "quarantined", operation: "op" })).toBe(
      "격리 공간으로 옮겼습니다(복원 가능).",
    );
    expect(outcomeText(ko, { kind: "skipped", reason: "outside_roots" })).toBe(
      "옮기지 않았습니다: 스캔한 폴더 밖에 있습니다.",
    );
    expect(outcomeText(ko, { kind: "failed", reason: "IdentityMismatch", left: "gone" })).toBe(
      "이동에 실패했습니다(IdentityMismatch). 항목이 사라졌습니다.",
    );
  });
});
