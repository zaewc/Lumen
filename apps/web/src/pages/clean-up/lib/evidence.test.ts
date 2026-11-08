import { describe, expect, it } from "vitest";
import { en, formatDateTime } from "@/shared/i18n";
import type { EvidenceLine } from "../model/stack";
import { lineText } from "./evidence";

const AT = "2026-10-16T09:00:00Z";

/** `lineText` in English for one line; source and certainty do not change the words. */
const text = (line: EvidenceLine) => lineText(en, "en", line);

describe("lineText", () => {
  it("says the policy's reasons in the catalog's words", () => {
    const base = { type: "reason", source: "rule", certainty: "resolved" } as const;
    expect(text({ ...base, reason: { rule: "protect.in-use", text: "server" } })).toBe(
      "Kept: a running app is using it.",
    );
    expect(text({ ...base, reason: { rule: "protect.location.Desktop", text: "server" } })).toBe(
      "Kept: it is in a protected location (Desktop).",
    );
    expect(text({ ...base, reason: { rule: "classify.unheard-of", text: "From the core." } })).toBe(
      "From the core.",
    );
  });

  it("says who owns it and whether it is open", () => {
    const owner = { type: "owner", app: "Spotify", source: "rule", certainty: "resolved" } as const;
    expect(text({ ...owner, present: true })).toBe("Belongs to Spotify, which is installed.");
    expect(text({ ...owner, present: false })).toBe("Belongs to Spotify, which is not installed.");
    expect(text({ ...owner, present: null })).toBe(
      "Belongs to Spotify. Whether it is installed is unknown.",
    );
    const running = { type: "app_running", app: "Spotify", source: "process" } as const;
    expect(text({ ...running, running: true, certainty: "resolved" })).toBe("Spotify is running.");
    expect(text({ ...running, running: false, certainty: "resolved" })).toBe(
      "Spotify is not running.",
    );
    expect(
      text({ type: "app_opened", app: "Spotify", at: AT, source: "time", certainty: "partial" }),
    ).toBe(
      `Spotify was last opened ${formatDateTime("en", AT)}; the system may not record every launch.`,
    );
    expect(text({ type: "project_activity", at: AT, source: "time", certainty: "partial" })).toBe(
      `Its project's files last changed ${formatDateTime("en", AT)}; only files like the manifest and lockfile count.`,
    );
    const inUse = { type: "in_use", source: "process", certainty: "resolved" } as const;
    expect(text({ ...inUse, inUse: true })).toBe("An app has it open.");
    expect(text({ ...inUse, inUse: false })).toBe("No app has it open.");
    const unseen = { type: "unseen", source: "process", certainty: "partial" } as const;
    expect(text({ ...unseen, unseen: null })).toBe("No visible app has it open.");
    expect(text({ ...unseen, unseen: "3" })).toBe(
      "No visible app has it open; 3 processes could not be checked.",
    );
  });

  it("says only as much about a time as the platform vouches for", () => {
    const date = formatDateTime("en", AT);
    const time = { type: "time", source: "time", folder: false } as const;
    expect(
      text({
        ...time,
        which: "modified",
        certainty: "resolved",
        time: { state: "recorded", at: AT },
      }),
    ).toBe(`Last changed ${date}.`);
    expect(
      text({
        ...time,
        folder: true,
        which: "modified",
        certainty: "resolved",
        time: { state: "recorded", at: AT },
      }),
    ).toBe(`The folder entry last changed ${date}; its contents may have changed later.`);
    expect(
      text({
        ...time,
        which: "accessed",
        certainty: "partial",
        time: { state: "may_lag", at: AT },
      }),
    ).toBe(`Last opened ${date} or later; the system may not record every use.`);
    expect(
      text({
        ...time,
        which: "accessed",
        certainty: "unknown",
        time: { state: "not_recorded" },
      }),
    ).toBe("No time of last use was recorded.");
    expect(
      text({ ...time, which: "modified", certainty: "unknown", time: { state: "not_read" } }),
    ).toBe("Time of last change unknown: not read.");
  });

  it("says each relation of the lens, with its size", () => {
    const related = { type: "related", source: "relation", certainty: "resolved" } as const;
    expect(
      text({ ...related, line: { relation: "tool", tool: "npm", count: 2, bytes: "1500000" } }),
    ).toBe("2 other items from npm · 1.5 MB");
    expect(
      text({
        ...related,
        line: { relation: "processes", processes: ["Slack", "Zoom"], count: 1, bytes: "999" },
      }),
    ).toBe("1 other item also in use by Slack and Zoom · 999 B");
    expect(text({ ...related, line: { relation: "evidence", count: 3, bytes: "2000" } })).toBe(
      "3 other items linked by what the scan recorded · 2.0 kB",
    );
    expect(
      text({ ...related, line: { relation: "kind", kind: "app_cache", count: 4, bytes: "0" } }),
    ).toBe("4 other items in App caches · 0 B");
    expect(text({ type: "unrelated", source: "relation", certainty: "unknown" })).toBe(
      "Nothing recorded links this to another item.",
    );
  });

  it("labels AI review's note as AI", () => {
    const note = { type: "note", source: "ai", certainty: "partial" } as const;
    expect(text({ ...note, note: { kind: "developer_tool_data", confidence: "high" } })).toBe(
      "AI: likely a developer tool's data (high confidence)",
    );
    expect(text({ ...note, note: { kind: "unknown", confidence: "low" } })).toBe(
      "AI: could not tell what this is",
    );
  });
});
