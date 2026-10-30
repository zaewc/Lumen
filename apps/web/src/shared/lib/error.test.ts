import { describe, expect, it } from "vitest";
import { errorMessage } from "./error";

describe("errorMessage", () => {
  it("reads an Error, a string or the host's { message }", () => {
    expect(errorMessage(new Error("disk unplugged"))).toBe("disk unplugged");
    expect(errorMessage("command scan not found")).toBe("command scan not found");
    expect(errorMessage({ message: "another command is still running" })).toBe(
      "another command is still running",
    );
  });

  it("has no words for empty or unrecognised values, never [object Object]", () => {
    for (const value of [
      undefined,
      null,
      42,
      {},
      { message: 7 },
      { reason: "x" },
      [],
      "  ",
      new Error(""),
      { message: "" },
    ]) {
      expect(errorMessage(value), JSON.stringify(value)).toBeNull();
    }
  });
});
