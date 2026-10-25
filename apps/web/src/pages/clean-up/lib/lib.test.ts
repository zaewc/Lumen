import { describe, expect, it } from "vitest";
import { en, ko } from "@/shared/i18n";
import { commandLine, shellWord } from "./command";
import { moveIndex, PAGE_STEP } from "./keys";
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

describe("commandLine", () => {
  it("joins plain words as they are", () => {
    expect(
      commandLine({
        tool: "Xcode",
        program: "xcrun",
        args: ["simctl", "delete", "unavailable"],
      }),
    ).toBe("xcrun simctl delete unavailable");
    expect(commandLine({ tool: "Go", program: "go", args: ["clean", "-modcache"] })).toBe(
      "go clean -modcache",
    );
    expect(commandLine({ tool: "Homebrew", program: "brew", args: [] })).toBe("brew");
  });

  it("single-quotes words a shell would read otherwise", () => {
    expect(shellWord("a b")).toBe("'a b'");
    expect(shellWord("")).toBe("''");
    expect(shellWord("$HOME")).toBe("'$HOME'");
    expect(shellWord("*.log")).toBe("'*.log'");
    expect(shellWord("a;rm")).toBe("'a;rm'");
    expect(shellWord("it's")).toBe("'it'\\''s'");
    expect(commandLine({ tool: "T", program: "my tool", args: ["--dir", "~/My Files"] })).toBe(
      "'my tool' --dir '~/My Files'",
    );
  });
});

describe("moveIndex", () => {
  it("moves by one with the arrow keys and stops at the edges", () => {
    expect(moveIndex(0, 3, "ArrowDown")).toBe(1);
    expect(moveIndex(2, 3, "ArrowDown")).toBe(2);
    expect(moveIndex(1, 3, "ArrowUp")).toBe(0);
    expect(moveIndex(0, 3, "ArrowUp")).toBe(0);
  });

  it("jumps a page or to either end", () => {
    expect(moveIndex(0, 40, "PageDown")).toBe(PAGE_STEP);
    expect(moveIndex(35, 40, "PageDown")).toBe(39);
    expect(moveIndex(5, 40, "PageUp")).toBe(0);
    expect(moveIndex(7, 40, "Home")).toBe(0);
    expect(moveIndex(7, 40, "End")).toBe(39);
  });

  it("brings a stale selection back into a shorter list", () => {
    expect(moveIndex(9, 3, "ArrowUp")).toBe(2);
  });

  it("leaves other keys and empty lists alone", () => {
    expect(moveIndex(0, 3, "Enter")).toBeNull();
    expect(moveIndex(0, 3, " ")).toBeNull();
    expect(moveIndex(0, 0, "ArrowDown")).toBeNull();
  });
});
