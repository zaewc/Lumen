import { describe, expect, it } from "vitest";
import { ariaShortcut, isCommand, type KeyPress, shortcutLabel } from "./shortcut";

const press = (key: string, modifiers: Partial<KeyPress> = {}): KeyPress => ({
  key,
  code: "",
  metaKey: false,
  ctrlKey: false,
  altKey: false,
  shiftKey: false,
  ...modifiers,
});

describe("isCommand", () => {
  it("takes ⌘ on a Mac and Ctrl elsewhere", () => {
    expect(isCommand(press("r", { metaKey: true }), "r", true)).toBe(true);
    expect(isCommand(press("r", { ctrlKey: true }), "r", true)).toBe(false);
    expect(isCommand(press("r", { ctrlKey: true }), "r", false)).toBe(true);
    expect(isCommand(press("r", { metaKey: true }), "r", false)).toBe(false);
  });

  it("matches letters in either case and digits as they are", () => {
    expect(isCommand(press("R", { metaKey: true }), "r", true)).toBe(true);
    expect(isCommand(press("2", { ctrlKey: true }), "2", false)).toBe(true);
    expect(isCommand(press("3", { ctrlKey: true }), "2", false)).toBe(false);
  });

  it("matches the physical key under another input source", () => {
    expect(isCommand(press("ㄱ", { code: "KeyR", metaKey: true }), "r", true)).toBe(true);
    expect(isCommand(press("!", { code: "Digit1", ctrlKey: true }), "1", false)).toBe(true);
    expect(isCommand(press("ㄱ", { code: "KeyT", metaKey: true }), "r", true)).toBe(false);
  });

  it("ignores the key alone and any extra modifier", () => {
    expect(isCommand(press("r"), "r", true)).toBe(false);
    expect(isCommand(press("r", { metaKey: true, shiftKey: true }), "r", true)).toBe(false);
    expect(isCommand(press("r", { metaKey: true, altKey: true }), "r", true)).toBe(false);
    expect(isCommand(press("r", { metaKey: true, ctrlKey: true }), "r", true)).toBe(false);
  });
});

describe("shortcut labels", () => {
  it("write the shortcut the platform's way", () => {
    expect(shortcutLabel("r", true)).toBe("⌘R");
    expect(shortcutLabel("1", false)).toBe("Ctrl+1");
    expect(ariaShortcut("r", true)).toBe("Meta+R");
    expect(ariaShortcut("4", false)).toBe("Control+4");
  });
});
