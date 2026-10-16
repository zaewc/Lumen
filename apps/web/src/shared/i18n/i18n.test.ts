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
      "정리 제안: 앱의 캐시라서 앱이 다시 만들어요.",
    );
  });

  it("name the protected location and fall back to the server for unknown rules", () => {
    const rule = "protect.location.macos-sip-usr";
    expect(reasonText(en, { rule, text: "" })).toBe(
      "Kept: it is in a protected location (macos-sip-usr).",
    );
    expect(reasonText(ko, { rule, text: "" })).toBe("유지: 보호된 위치(macos-sip-usr)에 있어요.");
    const unknown = { rule: "made.up", text: "Rule made.up applied." };
    expect(reasonText(ko, unknown)).toBe("Rule made.up applied.");
  });

  it("group digits in counts, exactly for decimal strings", () => {
    expect(countFormat("en")(1204)).toBe("1,204");
    expect(countFormat("ko")("18446744073709551615")).toBe("18,446,744,073,709,551,615");
    expect(countFormat("ko")("n/a")).toBe("n/a");
    expect(ko.confirm.title(1204, "4.2 GB")).toBe("항목 1,204개(4.2 GB)를 격리함으로 옮길까요?");
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
  const terms = ["verdicts", "quarantine"] as const satisfies readonly GlossaryTerm[];

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
    expect(ko.whatIs("영구 삭제")).toBe("‘영구 삭제’란?");
    expect(ko.whatIs("읽지 못한 폴더")).toBe("‘읽지 못한 폴더’란?");
    expect(ko.whatIs("복원")).toBe("‘복원’이란?");
    expect(ko.whatIs("Lumen")).toBe("‘Lumen’이란?");
  });
});

describe("plain words", () => {
  it("count items and days in each language", () => {
    expect(en.held.days(0)).toBe("Today");
    expect(en.held.days(1)).toBe("1 day");
    expect(en.held.days(1204)).toBe("1,204 days");
    expect(ko.held.days(30)).toBe("30일");
    expect(en.common.items(1)).toBe("1 item");
    expect(ko.common.items(2)).toBe("항목 2개");
  });
});

/** Every message in a catalog, with functions called on sample arguments. */
function texts(value: unknown, path = ""): [string, string][] {
  if (typeof value === "string") {
    return [[path, value]];
  }
  if (typeof value === "function") {
    return texts(value(...Array.from({ length: value.length }, () => "2")), path);
  }
  if (typeof value === "object" && value !== null) {
    return Object.entries(value).flatMap(([key, inner]) => texts(inner, `${path}.${key}`));
  }
  return [];
}

describe("Korean tone (docs/design/ux-writing.md)", () => {
  const all = texts(ko);

  it("speaks 해요체: no 합니다체 or plain -다 sentence endings", () => {
    for (const [path, text] of all) {
      expect(text, path).not.toMatch(/[가-힣]다(?=[.?!…]|$)/u);
      expect(text, path).not.toMatch(/니다|니까/u);
    }
  });

  it("uses no ‘~시’ honorifics, only ‘~해 주세요’ for requests", () => {
    for (const [path, text] of all) {
      expect(text, path).not.toMatch(/십시오|시겠|시나요|(?<!주)세요|께서/u);
    }
  });

  it("says 돼요 rather than 되어요, and never 취소", () => {
    for (const [path, text] of all) {
      expect(text, path).not.toMatch(/되어요|취소/u);
    }
  });

  it("names the action on every dialog's confirming button", () => {
    expect(ko.common.close).toBe("닫기");
    expect(en.common.close).toBe("Close");
    expect(ko.confirm.move).toBe("격리함으로 옮기기");
    expect(ko.held.removeForGood).toBe("영구 삭제");
    expect(ko.ai.consent.confirm).toBe("AI 검토 켜기");
    expect(en.ai.consent.confirm).toBe("Turn on AI review");
  });
});
