import type { AiSettingsView } from "@lumen/api-client";
import { describe, expect, it } from "vitest";
import {
  canTurnOn,
  destination,
  isOn,
  needsKey,
  offersOwnKey,
  serviceName,
  sourceOnConsent,
} from "./ai";

const off: AiSettingsView = { enabled: false, consented_at: null, key: "none", key_source: null };
const stored: AiSettingsView = { ...off, key: "jeb", key_source: "stored" };
const builtin: AiSettingsView = { ...off, key: "jeb", key_source: "builtin" };

describe("AI review", () => {
  it("names the service of the key in use, never a missing one", () => {
    expect(serviceName("jeb")).toBe("Jeb");
    expect(serviceName("openrouter")).toBe("OpenRouter");
    expect(serviceName("none")).toBeNull();
  });

  it("asks for a key only when there is none, and only accepts a key-shaped one", () => {
    expect(needsKey(off)).toBe(true);
    expect(canTurnOn(off, "")).toBe(false);
    expect(canTurnOn(off, "not a key")).toBe(false);
    expect(canTurnOn(off, " jeb-key ")).toBe(true);
    expect(needsKey(stored)).toBe(false);
    expect(canTurnOn(stored, "")).toBe(true);
  });

  it("needs no key with the built-in one, but offers to use the user's own", () => {
    expect(needsKey(builtin)).toBe(false);
    expect(offersOwnKey(builtin)).toBe(true);
    expect(offersOwnKey(stored)).toBe(false);
    expect(offersOwnKey(off)).toBe(false);
    expect(canTurnOn(builtin, "")).toBe(true);
    expect(canTurnOn(builtin, "not a key")).toBe(false);
    expect(canTurnOn(builtin, "jeb-key")).toBe(true);
    expect(sourceOnConsent(builtin, "")).toBe("builtin");
    expect(sourceOnConsent(builtin, "jeb-key")).toBe("stored");
    expect(sourceOnConsent(off, "jeb-key")).toBe("stored");
  });

  it("discloses Jeb unless an OpenRouter key is stored", () => {
    expect(destination(off)).toBe("jeb");
    expect(destination(builtin)).toBe("jeb");
    expect(destination(stored)).toBe("jeb");
    expect(destination({ ...stored, key: "openrouter" })).toBe("openrouter");
  });

  it("is on only when turned on with a key, and says whose key", () => {
    expect(isOn(off)).toBeNull();
    expect(isOn(stored)).toBeNull();
    expect(isOn({ ...stored, enabled: true })).toEqual({ service: "Jeb", source: "stored" });
    expect(isOn({ ...builtin, enabled: true })).toEqual({ service: "Jeb", source: "builtin" });
    expect(isOn({ ...off, enabled: true })).toBeNull();
  });
});
