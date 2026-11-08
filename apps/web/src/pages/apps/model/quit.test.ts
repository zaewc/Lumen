import type {
  UninstallAppView,
  UninstallPreviewView,
  UninstallReasonView,
} from "@lumen/api-client";
import { describe, expect, it } from "vitest";
import { mayBeRunning, onlyRunning, quitStage } from "./quit";

const app: UninstallAppView = {
  key: "mac_app:com.spotify.client",
  name: "Spotify",
  bundle_id: "com.spotify.client",
  version: "1.2.48",
  path: "/Applications/Spotify.app",
  size_bytes: "412000000",
  size_complete: true,
  notes: ["goes_to_quarantine", "library_data_stays"],
};

function refused(...reasons: UninstallReasonView[]): UninstallPreviewView {
  return { kind: "refused", app, reasons };
}

const ready: UninstallPreviewView = {
  kind: "ready",
  preview: "0190f0d2-6c1e-7a3b-8c4d-5e6f7081b001",
  hash: `b3:${"0".repeat(64)}`,
  created_at: "2026-11-08T00:00:00Z",
  app,
};

describe("quitting a running app (roadmap 16.3a)", () => {
  it("offers to quit only when running is the only reason", () => {
    expect(onlyRunning(refused("running"))).toBe(true);
    expect(onlyRunning(refused("running", "not_owned_by_user"))).toBe(false);
    expect(onlyRunning(refused("running_unknown"))).toBe(false);
    expect(onlyRunning(ready)).toBe(false);
    expect(onlyRunning(null)).toBe(false);
    expect(quitStage(refused("running"), null, false)).toBe("offer");
    expect(quitStage(refused("running", "has_helpers"), null, false)).toBe("none");
    expect(quitStage(ready, { kind: "asked" }, true)).toBe("none");
  });

  it("waits after asking, then says it still runs", () => {
    const running = refused("running");
    expect(quitStage(running, { kind: "asked" }, true)).toBe("waiting");
    expect(quitStage(running, { kind: "asked" }, false)).toBe("still_running");
    expect(quitStage(running, { kind: "not_running" }, false)).toBe("still_running");
  });

  it("explains Lumen itself and a request that could not be sent", () => {
    const running = refused("running");
    expect(quitStage(running, { kind: "itself" }, true)).toBe("itself");
    expect(quitStage(running, { kind: "failed" }, true)).toBe("failed");
    expect(quitStage(running, { kind: "not_found" }, false)).toBe("failed");
  });

  it("lets the person check again when the app runs or may run", () => {
    expect(mayBeRunning(refused("running_unknown"))).toBe(true);
    expect(mayBeRunning(refused("running", "protected"))).toBe(true);
    expect(mayBeRunning(refused("protected"))).toBe(false);
    expect(mayBeRunning(ready)).toBe(false);
  });
});
