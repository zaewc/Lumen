import type { ReasonView } from "@lumen/api-client";
import { en, type Messages } from "./en";
import { ko } from "./ko";
import type { Locale } from "./locale";
import { LOCATION_RULE_PREFIX, RULE_IDS } from "./rules";

/** Every locale's catalog. */
export const MESSAGES: { readonly [L in Locale]: Messages } = { en, ko };

/**
 * One policy reason in the catalog's language, chosen by its stable rule ID.
 * A rule this UI does not know keeps the server's (English) sentence.
 */
export function reasonText(messages: Messages, reason: ReasonView): string {
  if (reason.rule.startsWith(LOCATION_RULE_PREFIX)) {
    return messages.reasons.location(reason.rule.slice(LOCATION_RULE_PREFIX.length));
  }
  const rule = RULE_IDS.find((id) => id === reason.rule);
  return rule === undefined ? reason.text : messages.reasons.rules[rule];
}
