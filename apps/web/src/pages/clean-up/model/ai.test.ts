import type { AiSettingsView } from "@lumen/api-client";
import { describe, expect, it } from "vitest";
import { canTurnOn, destination, needsKey, serviceName } from "./ai";

const off: AiSettingsView = { enabled: false, consented_at: null, key: "none" };

describe("AI review", () => {
  it("names the service of the stored key, never a missing one", () => {
    expect(serviceName("jeb")).toBe("Jeb");
    expect(serviceName("openrouter")).toBe("OpenRouter");
    expect(serviceName("none")).toBeNull();
  });

  it("asks for a key only when none is stored, and only accepts a key-shaped one", () => {
    expect(needsKey(off)).toBe(true);
    expect(canTurnOn(off, "")).toBe(false);
    expect(canTurnOn(off, "not a key")).toBe(false);
    expect(canTurnOn(off, " jeb-key ")).toBe(true);
    const stored = { ...off, key: "jeb" } as const;
    expect(needsKey(stored)).toBe(false);
    expect(canTurnOn(stored, "")).toBe(true);
  });

  it("discloses Jeb unless an OpenRouter key is stored", () => {
    expect(destination(off)).toBe("jeb");
    expect(destination({ ...off, key: "jeb" })).toBe("jeb");
    expect(destination({ ...off, key: "openrouter" })).toBe("openrouter");
  });
});
