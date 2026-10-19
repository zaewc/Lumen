import {
  zCandidateView,
  zFinalizeOutcomeView,
  zHeldItemView,
  zPlanView,
  zQuarantineReportView,
  zRestoreOutcomeView,
  zScanProgressView,
  zScanView,
} from "@lumen/api-client";
import { describe, expect, it } from "vitest";
import { memoryTransport } from "./memory";
import { operationOf } from "./transport";

describe("memoryTransport", () => {
  it("answers with data the generated schemas accept", async () => {
    const transport = memoryTransport();
    const progress: unknown[] = [];
    zScanView.parse(await transport.scan({ roots: ["/Users/ana"] }, (p) => progress.push(p)));
    expect(progress.map((p) => zScanProgressView.parse(p).entries)).toEqual(["1580"]);
    for (const candidate of await transport.assess()) {
      zCandidateView.parse(candidate);
    }
    const plan = zPlanView.parse(await transport.plan());
    const report = await transport.execute({ plan: plan.plan, confirmation: plan.hash });
    expect(zQuarantineReportView.parse(report).moved_bytes).toBe("4200000000");
    const [item] = (await transport.held()).map((view) => zHeldItemView.parse(view));
    expect(item?.state).toBe("retained");
    if (item === undefined) {
      throw new Error("one held item expected");
    }
    const operation = operationOf(item);
    expect(zRestoreOutcomeView.parse(await transport.restore(operation)).outcome.kind).toBe(
      "restored",
    );
    expect(zFinalizeOutcomeView.parse(await transport.finalize(operation)).outcome.kind).toBe(
      "finalized",
    );
  });
});
