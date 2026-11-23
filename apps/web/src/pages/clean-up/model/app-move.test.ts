import type {
  UninstallAppView,
  UninstallPreviewView,
  UninstallReasonView,
} from "@lumen/api-client";
import { describe, expect, it } from "vitest";
import { helperWouldMove } from "./app-move";

function app(path: string): UninstallAppView {
  return {
    key: "mac_app:com.microsoft.Word",
    name: "Word",
    bundle_id: "com.microsoft.Word",
    version: "16.90",
    path,
    size_bytes: "2000000000",
    size_complete: true,
    notes: ["goes_to_quarantine", "library_data_stays"],
  };
}

function refused(path: string, ...reasons: UninstallReasonView[]): UninstallPreviewView {
  return { kind: "refused", app: app(path), reasons };
}

describe("the administrator helper hint (ADR-0063)", () => {
  it("shows only when another account owning an app in /Applications is the one refusal", () => {
    expect(helperWouldMove(refused("/Applications/Word.app", "not_owned_by_user"))).toBe(true);
  });

  it("stays hidden for any other refusal, folder or preview", () => {
    expect(helperWouldMove(refused("/Applications/Word.app", "not_owned_by_user", "running"))).toBe(
      false,
    );
    expect(helperWouldMove(refused("/Applications/Word.app", "owner_unknown"))).toBe(false);
    expect(helperWouldMove(refused("/Users/ana/Applications/Word.app", "not_owned_by_user"))).toBe(
      false,
    );
    expect(helperWouldMove(refused("/Applications/Tools/Word.app", "not_owned_by_user"))).toBe(
      false,
    );
    expect(
      helperWouldMove({
        kind: "ready",
        preview: "0190f0d2-6c1e-7a3b-8c4d-5e6f7081b001",
        hash: `b3:${"0".repeat(64)}`,
        created_at: "2026-11-23T00:00:00Z",
        app: app("/Applications/Word.app"),
        by_helper: true,
      }),
    ).toBe(false);
    expect(helperWouldMove({ kind: "not_found" })).toBe(false);
  });
});
