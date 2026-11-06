import type { AiNoteView } from "@lumen/api-client";
import { LOCATION_RULE_PREFIX, RULE_IDS, type RuleId } from "@/shared/i18n";

/**
 * How settled a fact is, drawn the same way everywhere
 * (docs/design/visual-language.md, Certainty): resolved facts are measured or
 * matched; partial ones were seen only in part or guessed; unknown ones were
 * not seen at all.
 */
export const CERTAINTIES = ["resolved", "partial", "unknown"] as const;

export type Certainty = (typeof CERTAINTIES)[number];

/** Every rule's certainty; a new rule fails the type check until it is here. */
const BY_RULE: { readonly [R in RuleId]: Certainty } = {
  "default.review": "unknown",
  "default.keep-small": "resolved",
  "protect.os-protected": "resolved",
  "protect.system-file": "resolved",
  "protect.cloud-backed": "resolved",
  "protect.in-use": "resolved",
  "protect.app-running": "resolved",
  "protect.in-use-partial": "partial",
  "protect.in-use-unknown": "unknown",
  "protect.not-observed": "unknown",
  "protect.incomplete-scan": "partial",
  "protect.location-data": "resolved",
  "protect.location-context": "resolved",
  "protect.contains-system": "resolved",
  "protect.contains-sensitive": "resolved",
  "protect.contains-cloud": "resolved",
  "eligible.quarantine-unsupported": "resolved",
  "eligible.frees-nothing": "resolved",
  "eligible.object-kind": "resolved",
  "eligible.in-use-partial": "partial",
  "eligible.in-use-partial-single-user": "partial",
  "classify.devkb-regenerable": "resolved",
  "classify.devkb-tool-action": "resolved",
  "classify.devkb-not-regenerable": "resolved",
  "classify.devkb-self-managed": "resolved",
  "classify.devkb-unknown": "unknown",
  "classify.vendor-declared-cache": "resolved",
  "classify.browser-profile": "resolved",
  "classify.app-cache": "resolved",
  "classify.app-state": "resolved",
  "classify.orphan": "resolved",
  // AI review's caution is a guess, never a measurement.
  "jev.demote": "partial",
};

/**
 * The certainty of a policy reason. A matched protected location is
 * resolved; a rule this interface does not know is unknown.
 */
export function certaintyOfRule(rule: string): Certainty {
  if (rule.startsWith(LOCATION_RULE_PREFIX)) {
    return "resolved";
  }
  const known = RULE_IDS.find((id) => id === rule);
  return known === undefined ? "unknown" : BY_RULE[known];
}

/** The certainty of AI review's guess at what an item is. */
export function certaintyOfNote(note: AiNoteView): Certainty {
  if (note.kind === "unknown") {
    return "unknown";
  }
  return note.confidence === "high" ? "resolved" : "partial";
}

/** The certainty of a size: exact, or only a lower bound. */
export function certaintyOfSize(exact: boolean): Certainty {
  return exact ? "resolved" : "partial";
}
