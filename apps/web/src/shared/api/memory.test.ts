import {
  zAiSettingsView,
  zCandidateView,
  zDiskView,
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
    const candidates = (await transport.assess()).map((c) => zCandidateView.parse(c));
    expect(candidates.find((c) => c.verdict === "review")?.tool_command?.program).toBe("xcrun");
    expect(
      candidates.filter((c) => c.verdict === "review" && c.tool_command === null).length,
    ).toBeGreaterThan(0);
    zDiskView.parse(await transport.disk());
    const plan = zPlanView.parse(await transport.plan());
    expect(plan.items.every((item) => item.verdict === "quarantine")).toBe(true);
    const report = zQuarantineReportView.parse(
      await transport.execute({ plan: plan.plan, confirmation: plan.hash }),
    );
    expect(report.items.map((item) => item.result.kind)).toEqual([
      "quarantined",
      "quarantined",
      "skipped",
      "quarantined",
    ]);
    expect(report.moved_bytes).toBe("8100000000");
    const held = (await transport.held()).map((view) => zHeldItemView.parse(view));
    expect(held).toHaveLength(5);
    const [first, second] = held;
    if (first === undefined || second === undefined) {
      throw new Error("held items expected");
    }
    expect(first.state).toBe("retained");
    expect(
      zRestoreOutcomeView.parse(await transport.restore(operationOf(first))).outcome.kind,
    ).toBe("restored");
    expect(
      zFinalizeOutcomeView.parse(await transport.finalize(operationOf(second))).outcome.kind,
    ).toBe("finalized");
    expect(await transport.held()).toHaveLength(3);
    expect((await transport.finalize(operationOf(second))).outcome.kind).toBe("not_found");
  });

  it("starts with AI review on, which needs a key before it can be turned on again", async () => {
    const transport = memoryTransport();
    expect(zAiSettingsView.parse(await transport.aiSettings())).toEqual({
      enabled: true,
      consented_at: null,
      key: "none",
      key_source: null,
    });
    await expect(transport.setAiReview(true)).rejects.toThrow();
    await expect(transport.setAiKey("not a key")).rejects.toThrow();
    expect((await transport.setAiKey(" jeb-key ")).key).toBe("jeb");
    const on = zAiSettingsView.parse(await transport.setAiReview(true));
    expect(on.enabled).toBe(true);
    expect(on.consented_at).not.toBeNull();
    const off = zAiSettingsView.parse(await transport.setAiReview(false));
    expect(off).toEqual({ ...on, enabled: false });
    expect(await transport.aiSettings()).toEqual(off);
  });

  it("adds AI notes to review items only while AI review is on, never changing a verdict", async () => {
    const transport = memoryTransport();
    const noKey = (await transport.assess()).map((c) => zCandidateView.parse(c));
    expect(noKey.every((c) => c.ai_note === null)).toBe(true);
    await transport.setAiKey("jeb-key");
    await transport.setAiReview(false);
    const off = (await transport.assess()).map((c) => zCandidateView.parse(c));
    expect(off.every((c) => c.ai_note === null)).toBe(true);
    await transport.setAiReview(true);
    const on = (await transport.assess()).map((c) => zCandidateView.parse(c));
    expect(on.map((c) => c.verdict)).toEqual(off.map((c) => c.verdict));
    const noted = on.filter((c) => c.ai_note !== null);
    expect(noted.length).toBeGreaterThan(0);
    expect(noted.every((c) => c.verdict === "review" && c.tool_command === null)).toBe(true);
  });

  it("turns AI review on with the built-in key, and lets a stored key replace it", async () => {
    const transport = memoryTransport({ builtinKey: true });
    expect(await transport.aiSettings()).toEqual({
      enabled: true,
      consented_at: null,
      key: "jeb",
      key_source: "builtin",
    });
    const on = zAiSettingsView.parse(await transport.setAiReview(true));
    expect(on.enabled).toBe(true);
    expect(on.key_source).toBe("builtin");
    expect((await transport.setAiKey("jeb-key")).key_source).toBe("stored");
  });

  it("paces the scan for people, reporting growing totals", async () => {
    const transport = memoryTransport({ pace: 1 });
    const entries: string[] = [];
    await transport.scan({ roots: [] }, (p) => entries.push(zScanProgressView.parse(p).entries));
    expect(entries).toEqual(["310", "940", "1580"]);
    const verdicts = new Set(
      (await transport.assess()).map((c) => zCandidateView.parse(c).verdict),
    );
    expect(verdicts).toEqual(new Set(["quarantine", "review", "keep"]));
  });
});
