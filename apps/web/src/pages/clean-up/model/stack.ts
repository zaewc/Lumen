import type { AiNoteView, CandidateView, ReasonView, TimeView } from "@lumen/api-client";
import {
  type Certainty,
  certaintyOfNote,
  certaintyOfRule,
  statusOf,
} from "@/entities/scan-session";
import { LOCATION_RULE_PREFIX } from "@/shared/i18n";
import type { LensLine } from "./lens";

/** Where a line of evidence comes from, shown beside it in small capitals. */
export type EvidenceSource = "rule" | "location" | "time" | "relation" | "process" | "ai";

/**
 * One line of the inspector's evidence stack (docs/design/visual-language.md):
 * what it says, where it comes from, and how certain it is.
 */
export type EvidenceLine = {
  readonly source: EvidenceSource;
  readonly certainty: Certainty;
} & (
  | { readonly type: "reason"; readonly reason: ReasonView }
  | {
      readonly type: "owner";
      readonly app: string;
      /** Whether it is installed; `null` when nothing answered. */
      readonly present: boolean | null;
    }
  | { readonly type: "in_use"; readonly inUse: boolean }
  /** No visible process has it open; `unseen` processes could not be checked. */
  | { readonly type: "unseen"; readonly unseen: string | null }
  | {
      readonly type: "time";
      readonly which: "modified" | "accessed";
      readonly time: TimeView;
      readonly folder: boolean;
    }
  | { readonly type: "related"; readonly line: LensLine }
  /** Nothing recorded links it to another item: not "unrelated". */
  | { readonly type: "unrelated" }
  | { readonly type: "note"; readonly note: AiNoteView }
);

function timeCertainty(time: TimeView): Certainty {
  switch (time.state) {
    case "recorded":
      return "resolved";
    case "may_lag":
      return "partial";
    case "not_recorded":
    case "not_read":
      return "unknown";
  }
}

/**
 * The evidence behind `candidate`'s decision, in reading order: the rules
 * that fired, who owns it, whether it is in use, its times, what it is
 * related to (`related`, from the lens), and AI review's note. Only what the
 * API reports; a question nothing answered is Unknown, never "no" (ADR-0053).
 */
export function evidenceStack(
  candidate: CandidateView,
  related: readonly LensLine[],
): EvidenceLine[] {
  const lines: EvidenceLine[] = candidate.reasons.map((reason) => ({
    type: "reason",
    reason,
    source:
      reason.rule === "jev.demote"
        ? "ai"
        : reason.rule.startsWith(LOCATION_RULE_PREFIX)
          ? "location"
          : "rule",
    certainty: certaintyOfRule(reason.rule),
  }));

  const facts = candidate.facts.map((record) => record.fact);
  for (const fact of facts) {
    if (fact.type !== "owning_application") continue;
    const answer = facts.find((f) => f.type === "application_present" && f.app === fact.app);
    const present = answer?.type === "application_present" ? answer.present : null;
    lines.push({
      type: "owner",
      app: fact.app,
      present,
      source: "relation",
      certainty: present === null ? "partial" : "resolved",
    });
  }
  for (const fact of facts) {
    if (fact.type === "in_use") {
      lines.push({ type: "in_use", inUse: fact.in_use, source: "process", certainty: "resolved" });
    } else if (fact.type === "not_in_use_by_visible_processes") {
      const unseen = fact.unseen === null || fact.unseen === "0" ? null : fact.unseen;
      lines.push({
        type: "unseen",
        unseen,
        source: "process",
        certainty: unseen === null ? "resolved" : "partial",
      });
    }
  }

  const { times } = candidate;
  for (const which of ["modified", "accessed"] as const) {
    const time = times[which];
    lines.push({
      type: "time",
      which,
      time,
      folder: times.folder,
      source: "time",
      certainty: timeCertainty(time),
    });
  }

  for (const line of related) {
    lines.push({ type: "related", line, source: "relation", certainty: "resolved" });
  }
  if (related.length === 0) {
    lines.push({ type: "unrelated", source: "relation", certainty: "unknown" });
  }
  // An AI note is only ever on an item to review (ADR-0041).
  if (candidate.ai_note !== null && statusOf(candidate) === "review") {
    lines.push({
      type: "note",
      note: candidate.ai_note,
      source: "ai",
      certainty: certaintyOfNote(candidate.ai_note),
    });
  }
  return lines;
}
