import type { HeldItemView } from "@lumen/api-client";
import { describe, expect, it } from "vitest";
import { heldTotal } from "./total";

const item = (reclaim_bytes: string | null): HeldItemView => ({
  operation: "0190f0d2-6c1e-7a3b-8c4d-5e6f708192a3",
  state: "retained",
  original: "/Users/ana/Library/Caches/App",
  since: "2026-10-01T09:00:00Z",
  reclaim_bytes,
});

describe("heldTotal", () => {
  it("adds known sizes exactly and says when some are unknown", () => {
    expect(heldTotal([])).toEqual({ bytes: "0", exact: true });
    expect(heldTotal([item("4200000000"), item("18446744073709551615")])).toEqual({
      bytes: "18446744077909551615",
      exact: true,
    });
    expect(heldTotal([item("100"), item(null)])).toEqual({ bytes: "100", exact: false });
  });
});
