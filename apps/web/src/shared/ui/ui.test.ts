import { describe, expect, it } from "vitest";
import { type ButtonVariant, buttonClass } from "./button";
import { dialogChange } from "./modal";

describe("buttonClass", () => {
  const variants = [
    "primary",
    "secondary",
    "quiet",
    "danger",
    "dangerQuiet",
  ] as const satisfies readonly ButtonVariant[];

  it("keeps red for the irreversible variants only", () => {
    for (const variant of variants) {
      const red = buttonClass(variant).includes("red-");
      expect(red, variant).toBe(variant === "danger" || variant === "dangerQuiet");
    }
  });

  it("always shows a keyboard focus ring", () => {
    for (const variant of variants) {
      expect(buttonClass(variant, "lg")).toContain("focus-visible:outline-2");
    }
  });
});

describe("dialogChange", () => {
  it("shows or closes the element only when it is out of step", () => {
    expect(dialogChange(true, false)).toBe("show");
    expect(dialogChange(false, true)).toBe("close");
    expect(dialogChange(true, true)).toBe("none");
    expect(dialogChange(false, false)).toBe("none");
  });
});
