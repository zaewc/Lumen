import { describe, expect, it } from "vitest";
import { RULE_IDS } from "@/shared/i18n";
import { CERTAINTIES, certaintyOfNote, certaintyOfRule, certaintyOfSize } from "./certainty";

describe("certaintyOfRule", () => {
  it("gives every known rule a certainty", () => {
    for (const rule of RULE_IDS) {
      expect(CERTAINTIES).toContain(certaintyOfRule(rule));
    }
  });

  it("marks what was seen only in part as partial", () => {
    for (const rule of [
      "protect.in-use-partial",
      "protect.incomplete-scan",
      "eligible.in-use-partial",
      "eligible.in-use-partial-single-user",
      "jev.demote",
    ]) {
      expect(certaintyOfRule(rule)).toBe("partial");
    }
  });

  it("marks what was not seen as unknown", () => {
    for (const rule of [
      "default.review",
      "protect.in-use-unknown",
      "protect.not-observed",
      "classify.devkb-unknown",
    ]) {
      expect(certaintyOfRule(rule)).toBe("unknown");
    }
  });

  it("marks matched rules and protected locations as resolved", () => {
    expect(certaintyOfRule("classify.devkb-regenerable")).toBe("resolved");
    expect(certaintyOfRule("protect.in-use")).toBe("resolved");
    expect(certaintyOfRule("protect.location.documents")).toBe("resolved");
  });

  it("treats a rule it does not know as unknown", () => {
    expect(certaintyOfRule("classify.something-new")).toBe("unknown");
  });
});

describe("certaintyOfNote", () => {
  it("follows AI review's confidence", () => {
    expect(certaintyOfNote({ kind: "cache", confidence: "high" })).toBe("resolved");
    expect(certaintyOfNote({ kind: "cache", confidence: "medium" })).toBe("partial");
    expect(certaintyOfNote({ kind: "cache", confidence: "low" })).toBe("partial");
  });

  it("treats an unknown kind as unknown, however sure", () => {
    expect(certaintyOfNote({ kind: "unknown", confidence: "high" })).toBe("unknown");
  });
});

describe("certaintyOfSize", () => {
  it("treats a lower bound as partial", () => {
    expect(certaintyOfSize(true)).toBe("resolved");
    expect(certaintyOfSize(false)).toBe("partial");
  });
});
