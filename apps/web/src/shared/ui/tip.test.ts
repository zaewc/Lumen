import { describe, expect, it } from "vitest";
import { nextTip, panelShift } from "./tip";

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

describe("panelShift", () => {
  it("keeps the panel under its button when it fits", () => {
    expect(panelShift(100, 384, 1280)).toBe(0);
    expect(panelShift(880, 384, 1280)).toBe(0);
  });

  it("slides the panel left just enough to stay inside the viewport", () => {
    expect(panelShift(1200, 384, 1280)).toBe(-320);
    expect(panelShift(300, 328, 360)).toBe(-284);
  });

  it("never pushes the panel past the left edge", () => {
    expect(panelShift(0, 328, 360)).toBe(16);
    expect(panelShift(100, 500, 360)).toBe(-84);
  });
});
