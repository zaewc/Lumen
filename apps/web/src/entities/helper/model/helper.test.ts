import type { HelperView } from "@lumen/api-client";
import { describe, expect, it } from "vitest";
import { helperActions, helperLine } from "./helper";

const notAsked = { kind: "not_asked" } as const;

function view(registration: HelperView["registration"], reach: HelperView["reach"] = notAsked) {
  return { registration, reach } satisfies HelperView;
}

describe("the helper's Settings row (ADR-0063)", () => {
  it("is hidden where there is no helper", () => {
    expect(helperLine(view("unsupported"))).toBeNull();
    expect(helperActions(view("unsupported"))).toEqual([]);
  });

  it("offers to turn it on, then to allow it in Login Items", () => {
    expect(helperLine(view("not_registered"))).toEqual({ kind: "off" });
    expect(helperActions(view("not_registered"))).toEqual(["enable"]);
    expect(helperLine(view("requires_approval"))).toEqual({ kind: "waiting" });
    expect(helperActions(view("requires_approval"))).toEqual(["open_login_items", "disable"]);
  });

  it("is on only when the helper answers with this version", () => {
    const current = view("enabled", { kind: "answered", version: "0.11.0", current: true });
    expect(helperLine(current)).toEqual({ kind: "on", version: "0.11.0" });
    expect(helperActions(current)).toEqual(["disable"]);
    const old = view("enabled", { kind: "answered", version: "0.10.0", current: false });
    expect(helperLine(old)).toEqual({ kind: "problem", problem: "outdated" });
  });

  it("names each problem", () => {
    expect(helperLine(view("enabled", { kind: "unavailable" }))).toEqual({
      kind: "problem",
      problem: "no_answer",
    });
    expect(helperLine(view("enabled", { kind: "not_signed_as_required" }))).toEqual({
      kind: "problem",
      problem: "mismatch",
    });
    expect(helperLine(view("enabled", { kind: "unsigned" }))).toEqual({
      kind: "problem",
      problem: "unsigned",
    });
    expect(helperLine(view("not_found"))).toEqual({ kind: "problem", problem: "not_found" });
    expect(helperActions(view("not_found"))).toEqual(["enable"]);
  });
});
