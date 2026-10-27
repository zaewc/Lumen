/**
 * Every policy rule ID with a fixed explanation (`template` in
 * crates/lumen-policy/src/explain.rs), except the `protect.location.<name>`
 * family, which the catalogs render with its location name.
 */
export const RULE_IDS = [
  "default.review",
  "default.keep-small",
  "protect.os-protected",
  "protect.system-file",
  "protect.cloud-backed",
  "protect.in-use",
  "protect.in-use-partial",
  "protect.in-use-unknown",
  "protect.not-observed",
  "protect.incomplete-scan",
  "protect.location-data",
  "protect.location-context",
  "eligible.quarantine-unsupported",
  "eligible.frees-nothing",
  "eligible.object-kind",
  "eligible.in-use-partial",
  "eligible.in-use-partial-single-user",
  "classify.devkb-regenerable",
  "classify.devkb-tool-action",
  "classify.devkb-not-regenerable",
  "classify.devkb-self-managed",
  "classify.devkb-unknown",
  "classify.vendor-declared-cache",
  "classify.app-cache",
  "classify.app-state",
  "classify.orphan",
  "jev.demote",
] as const;

export type RuleId = (typeof RULE_IDS)[number];

/** The prefix of rules that name the protected location that matched. */
export const LOCATION_RULE_PREFIX = "protect.location.";
