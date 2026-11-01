import { describe, expect, it, vi } from "vitest";
import { followQuitHeld } from "./moving";

describe("quit held", () => {
  it("follows the host's held quit requests until stopped", () => {
    const target = new EventTarget();
    const held = vi.fn();
    const stop = followQuitHeld(target, held);
    target.dispatchEvent(new Event("lumen:window-focus"));
    expect(held).not.toHaveBeenCalled();
    target.dispatchEvent(new Event("lumen:quit-held"));
    target.dispatchEvent(new Event("lumen:quit-held"));
    expect(held).toHaveBeenCalledTimes(2);
    stop();
    target.dispatchEvent(new Event("lumen:quit-held"));
    expect(held).toHaveBeenCalledTimes(2);
  });
});
