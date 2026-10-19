import { describe, expect, it } from "vitest";
import { nextTip } from "./tip";

describe("nextTip", () => {
  it("toggles open and closed with its button", () => {
    expect(nextTip(false, "toggle")).toEqual({ open: true, focusButton: false });
    expect(nextTip(true, "toggle")).toEqual({ open: false, focusButton: false });
  });

  it("closes on Escape and returns focus only when it was open", () => {
    expect(nextTip(true, "escape")).toEqual({ open: false, focusButton: true });
    expect(nextTip(false, "escape")).toEqual({ open: false, focusButton: false });
  });

  it("closes on a click outside without moving focus", () => {
    expect(nextTip(true, "outside")).toEqual({ open: false, focusButton: false });
    expect(nextTip(false, "outside")).toEqual({ open: false, focusButton: false });
  });
});
