import { afterEach, describe, expect, it, vi } from "vitest";
import explainSource from "../../../../../crates/lumen-policy/src/explain.rs?raw";
import { en, type GlossaryTerm, type Messages } from "./en";
import { countFormat, formatDateTime } from "./format";
import { ko } from "./ko";
import { detectLocale, LOCALES } from "./locale";
import { MESSAGES, reasonText } from "./messages";
import { RULE_IDS } from "./rules";
import { storedLocale, storeLocale } from "./storage";

/** Every key path of a catalog, with each message's parameter count. */
function shape(value: unknown, path = ""): string[] {
  if (typeof value === "function") {
    return [`${path}(${value.length})`];
  }
  if (typeof value === "object" && value !== null) {
    return Object.entries(value).flatMap(([key, inner]) => shape(inner, `${path}.${key}`));
  }
  return [path];
}

describe("catalogs", () => {
  it("have the same keys and parameters in every language", () => {
    for (const locale of LOCALES) {
      expect(shape(MESSAGES[locale])).toEqual(shape(en));
    }
  });

  it("cover every rule explain.rs has a sentence for", () => {
    const template = explainSource.slice(0, explainSource.indexOf("pub fn explain"));
    const ids = [...template.matchAll(/"([a-z]+\.[a-z0-9.-]+)"\s*(?:=>|\|)/g)].map(([, id]) => id);
    expect(new Set(ids)).toEqual(new Set(RULE_IDS));
  });

  it("render every known rule in each language", () => {
    for (const messages of [en, ko] satisfies Messages[]) {
      for (const rule of RULE_IDS) {
        const text = reasonText(messages, { rule, text: "server" });
        expect(text, rule).not.toBe("server");
        expect(text.length, rule).toBeGreaterThan(0);
      }
    }
    expect(reasonText(ko, { rule: "classify.app-cache", text: "" })).toBe(
      "격리 제안: 앱의 캐시이며, 앱이 다시 만듭니다.",
    );
  });

  it("name the protected location and fall back to the server for unknown rules", () => {
    const rule = "protect.location.macos-sip-usr";
    expect(reasonText(en, { rule, text: "" })).toBe(
      "Kept: it is in a protected location (macos-sip-usr).",
    );
    expect(reasonText(ko, { rule, text: "" })).toBe("유지: 보호된 위치(macos-sip-usr)에 있습니다.");
    const unknown = { rule: "made.up", text: "Rule made.up applied." };
    expect(reasonText(ko, unknown)).toBe("Rule made.up applied.");
  });

  it("group digits in counts, exactly for decimal strings", () => {
    expect(countFormat("en")(1204)).toBe("1,204");
    expect(countFormat("ko")("18446744073709551615")).toBe("18,446,744,073,709,551,615");
    expect(countFormat("ko")("n/a")).toBe("n/a");
    expect(ko.confirm.title(1204, "4.2 GB")).toBe(
      "항목 1,204개(4.2 GB)를 격리함으로 옮기시겠습니까?",
    );
    expect(en.confirm.title(1, "4.2 GB")).toBe("Move 1 item (4.2 GB) to Quarantine?");
    expect(en.done.title(1204, "9.2 GB")).toBe("Moved 1,204 items (9.2 GB) to Quarantine.");
  });

  it("show timestamps as a date and time in each language", () => {
    expect(formatDateTime("en", "2026-10-16T09:00:00Z", "UTC")).toBe("Oct 16, 2026, 9:00 AM");
    expect(formatDateTime("ko", "2026-10-16T09:00:00Z", "UTC")).toBe("2026. 10. 16. 오전 9:00");
    expect(formatDateTime("en", "2026-10-16T18:30:00.5+09:00", "UTC")).toBe(
      "Oct 16, 2026, 9:30 AM",
    );
  });

  it("show anything that is not an RFC 3339 timestamp as it came", () => {
    for (const raw of ["", "n/a", "1", "2026-10-16", "2026-99-99T99:99:99Z"]) {
      expect(formatDateTime("en", raw)).toBe(raw);
      expect(formatDateTime("ko", raw)).toBe(raw);
    }
  });
});

describe("detectLocale", () => {
  it("takes the first supported language the user prefers", () => {
    expect(detectLocale(["ko-KR", "en"])).toBe("ko");
    expect(detectLocale(["fr", "ko"])).toBe("ko");
    expect(detectLocale(["en-US", "ko-KR"])).toBe("en");
    expect(detectLocale(["fr"])).toBe("en");
    expect(detectLocale(["kok"])).toBe("en");
    expect(detectLocale([])).toBe("en");
  });
});

describe("locale storage", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("works when storage throws", () => {
    const blocked = {
      getItem: () => {
        throw new Error("blocked");
      },
      setItem: () => {
        throw new Error("blocked");
      },
    };
    vi.stubGlobal("localStorage", blocked);
    expect(() => storeLocale("ko")).not.toThrow();
    expect(storedLocale()).toBeNull();
  });

  it("remembers a valid choice and ignores anything else", () => {
    const items = new Map<string, string>();
    vi.stubGlobal("localStorage", {
      getItem: (key: string) => items.get(key) ?? null,
      setItem: (key: string, value: string) => items.set(key, value),
    });
    expect(storedLocale()).toBeNull();
    storeLocale("ko");
    expect(storedLocale()).toBe("ko");
    items.set("lumen.locale", "fr");
    expect(storedLocale()).toBeNull();
  });
});

describe("glossary", () => {
  const terms = [
    "scan",
    "coverage",
    "reclaimable",
    "verdicts",
    "quarantine",
    "finalize",
    "restore",
  ] as const satisfies readonly GlossaryTerm[];

  it("explains every term in every language", () => {
    expect(Object.keys(en.glossary).sort()).toEqual([...terms].sort());
    for (const locale of LOCALES) {
      for (const term of terms) {
        const { title, body } = MESSAGES[locale].glossary[term];
        expect(title.trim(), `${locale} ${term}`).not.toBe("");
        expect(body.trim().length, `${locale} ${term}`).toBeGreaterThan(title.length);
      }
    }
  });

  it("asks about each term naturally", () => {
    expect(en.whatIs(en.glossary.quarantine.title)).toBe("What does “Quarantine” mean?");
    expect(ko.whatIs(ko.glossary.quarantine.title)).toBe("‘격리함’이란?");
    expect(ko.whatIs(ko.glossary.finalize.title)).toBe("‘영구 삭제’란?");
    expect(ko.whatIs(ko.glossary.coverage.title)).toBe("‘읽지 못한 폴더’란?");
    expect(ko.whatIs(ko.glossary.restore.title)).toBe("‘복원’이란?");
    expect(ko.whatIs("Lumen")).toBe("‘Lumen’이란?");
  });
});

describe("plain words", () => {
  it("count items and days in each language", () => {
    expect(en.held.days(0)).toBe("Moved here today");
    expect(en.held.days(1)).toBe("In Quarantine for 1 day");
    expect(en.held.days(30)).toBe("In Quarantine for 30 days");
    expect(ko.held.days(30)).toBe("보관한 지 30일");
    expect(en.quarantine.count(1)).toBe("1 item held");
    expect(ko.quarantine.count(2)).toBe("보관 중인 항목 2개");
  });
});
