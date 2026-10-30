import { describe, expect, it } from "vitest";
import { HostError, hostError } from "./host-error";

describe("hostError", () => {
  it("turns the host's CommandError into an Error with its message", () => {
    const rejection = { message: "AI review needs an API key" };
    const error = hostError(rejection);
    expect(error).toBeInstanceOf(HostError);
    expect(error.message).toBe("AI review needs an API key");
    expect(error.cause).toBe(rejection);
    expect(String(error)).toBe("HostError: AI review needs an API key");
  });

  it("keeps Tauri's own string rejections as the message", () => {
    expect(hostError("command finalize not found").message).toBe("command finalize not found");
  });

  it("passes real Errors through unchanged", () => {
    const original = new TypeError("bad response");
    expect(hostError(original)).toBe(original);
  });

  it("keeps no message for unrecognised rejections", () => {
    for (const rejection of [null, 3, { message: 3 }, { code: "E" }]) {
      const error = hostError(rejection);
      expect(error.message).toBe("");
      expect(error.cause).toBe(rejection);
    }
  });
});
