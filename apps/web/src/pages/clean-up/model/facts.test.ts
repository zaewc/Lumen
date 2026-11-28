import type { SoftwareItemView } from "@lumen/api-client";
import { describe, expect, it } from "vitest";
import { MESSAGES } from "@/shared/i18n";
import { removalFacts } from "./facts";

const en = MESSAGES.en;

function row(extra: Partial<SoftwareItemView> = {}): SoftwareItemView {
  return {
    key: "homebrew:openssl@3",
    origin: "homebrew",
    id: "openssl@3",
    name: null,
    version: "3.5.0",
    vendor: null,
    location: null,
    protected: false,
    hidden: false,
    explicit: false,
    services: [],
    facts: {
      mechanism: null,
      installed_by: null,
      needs_administrator: false,
      needs: null,
      needed_by: null,
    },
    relations: [],
    relations_read: null,
    ai_note: null,
    size: null,
    ...extra,
  };
}

const texts = (item: SoftwareItemView) =>
  removalFacts(en, item).map((fact) => `${fact.text} | ${fact.source}`);

describe("Lumen's removal facts", () => {
  it("says nothing it has no record of", () => {
    expect(removalFacts(en, row())).toEqual([]);
  });

  it("names each fact's source, from the package's own record", () => {
    const item = row({
      facts: {
        mechanism: "package_manager",
        installed_by: null,
        needs_administrator: false,
        needs: { count: 1, complete: true },
        needed_by: { count: 2, complete: true },
      },
    });
    expect(texts(item)).toEqual([
      "Its package manager removes it. | from Homebrew",
      "2 other packages need it. | from Homebrew",
      "It needs 1 other package. | from Homebrew",
    ]);
  });

  it("never reads unknown dependents as none", () => {
    const needed = (count: number, complete: boolean) =>
      row({ facts: { ...row().facts, needed_by: { count, complete } } });
    expect(texts(needed(0, true))).toEqual([
      "No other installed package needs it. | from Homebrew",
    ]);
    expect(texts(needed(0, false))).toEqual([
      "Lumen could not read what needs it. | from Homebrew",
    ]);
    expect(texts(needed(3, false))).toEqual(["At least 3 other packages need it. | from Homebrew"]);
  });

  it("leaves what needs a package to its relations section when that is shown", () => {
    const pkg = (complete: boolean, relations: SoftwareItemView["relations"]) =>
      row({
        relations_read: true,
        relations,
        facts: { ...row().facts, needed_by: { count: 0, complete } },
      });
    const candidate: SoftwareItemView["relations"][number] = {
      kind: "needed_by",
      other: "homebrew:act",
      label: "act",
      method: "homebrew_runtime_dependencies",
      direct: null,
      verified: false,
      ambiguous: true,
    };
    // A candidate is listed as "at least 1": no "could not read" beside it.
    expect(texts(pkg(false, [candidate]))).toEqual([]);
    // Unknown and nothing listed: the section says it is unknown.
    expect(texts(pkg(false, []))).toEqual([]);
    // Every record read and none needs it: only this line says so.
    expect(texts(pkg(true, []))).toEqual(["No other installed package needs it. | from Homebrew"]);
  });

  it("credits the package that installed an app, and Windows entries", () => {
    const app = row({
      origin: "mac_app",
      facts: {
        mechanism: "none_recorded",
        installed_by: "mac_installer",
        needs_administrator: true,
        needs: null,
        needed_by: null,
      },
    });
    expect(texts(app)).toEqual([
      "A package installed it. | from Installer package",
      "No uninstaller is recorded for it. | from Installer package",
      "Removing it needs administrator rights. | from Installer package",
    ]);
    const program = row({
      origin: "windows_program",
      facts: { ...row().facts, mechanism: "own_uninstaller" },
    });
    expect(texts(program)).toEqual(["It has its own uninstaller. | from its uninstall entry"]);
  });

  it("counts only verified background items, and never reads the AI note", () => {
    const service = (verified: boolean) =>
      ({
        kind: "launch_agent",
        label: "com.example.updater",
        scope: "user",
        method: "label_matches_bundle_id",
        verified,
        ambiguous: false,
      }) as const;
    const item = row({
      services: [service(true), service(false)],
      ai_note: {
        purpose: "runtime_or_library",
        removal_impact: "other_software_depends",
        purpose_confidence: "high",
        removal_impact_confidence: "high",
      },
    });
    expect(texts(item)).toEqual(["1 background item runs with it. | from its background items"]);
  });

  it("has every string in every locale", () => {
    const item = row({
      facts: {
        mechanism: "windows_installer",
        installed_by: null,
        needs_administrator: true,
        needs: { count: 2, complete: true },
        needed_by: { count: 1, complete: false },
      },
    });
    for (const messages of Object.values(MESSAGES)) {
      const facts = removalFacts(messages, item);
      expect(facts).toHaveLength(4);
      for (const fact of facts) {
        expect(fact.text.length).toBeGreaterThan(0);
        expect(fact.source).toContain("Homebrew");
      }
    }
  });
});
