import { describe, expect, it } from "vitest";
import { rootsOf } from "./ScanForm";

describe("rootsOf", () => {
  it("takes one trimmed root per non-empty line", () => {
    expect(rootsOf(" /a \n\n/b\n  \n")).toEqual(["/a", "/b"]);
    expect(rootsOf("")).toEqual([]);
  });
});
