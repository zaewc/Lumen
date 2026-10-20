import { describe, expect, it } from "vitest";
import { en, ko } from "@/shared/i18n";
import { outcomeText } from "./outcome";
import { rootsOf } from "./roots";

describe("rootsOf", () => {
  it("takes one trimmed folder per non-empty line", () => {
    expect(rootsOf(" /a \n\n/b\n  \n")).toEqual(["/a", "/b"]);
    expect(rootsOf("C:\\Users\\ana\r\nD:\\")).toEqual(["C:\\Users\\ana", "D:\\"]);
    expect(rootsOf("")).toEqual([]);
  });
});

describe("outcomeText", () => {
  it("says where every item is", () => {
    expect(outcomeText(en, { kind: "quarantined", operation: "op" })).toContain("restored");
    expect(outcomeText(en, { kind: "skipped", reason: "now_review" })).toContain("review");
    expect(
      outcomeText(en, { kind: "failed", reason: "IdentityMismatch", left: "source" }),
    ).toContain("untouched");
  });

  it("says it in Korean, keeping the core's failure code", () => {
    expect(outcomeText(ko, { kind: "quarantined", operation: "op" })).toBe(
      "격리함으로 옮겼습니다(복원 가능).",
    );
    expect(outcomeText(ko, { kind: "skipped", reason: "outside_roots" })).toBe(
      "옮기지 않았습니다: 검사한 폴더 밖에 있습니다.",
    );
    expect(outcomeText(ko, { kind: "failed", reason: "IdentityMismatch", left: "gone" })).toBe(
      "옮기지 못했습니다(IdentityMismatch). 항목이 사라졌습니다.",
    );
  });
});
