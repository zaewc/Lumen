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
  | { readonly type: "review_reason"; readonly code: string }
  | {
      readonly type: "owner";
      readonly app: string;
      /** Whether it is installed; `null` when nothing answered. */
      readonly present: boolean | null;
    }
  /** Whether an app that owns it is running (ADR-0059). */
  | { readonly type: "app_running"; readonly app: string; readonly running: boolean }
  | { readonly type: "in_use"; readonly inUse: boolean }
  /** No visible process has it open; `unseen` processes could not be checked. */
  | { readonly type: "unseen"; readonly unseen: string | null }
  | {
      readonly type: "time";
      readonly which: "modified" | "accessed";
      readonly time: TimeView;
      readonly folder: boolean;
    }
  /** When the system last recorded an owning app being opened (ADR-0059). */
  | { readonly type: "app_opened"; readonly app: string; readonly at: string }
  /** When its project's marker files last changed (ADR-0059). */
  | { readonly type: "project_activity"; readonly at: string }
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
 * that fired, who owns it, whether the owner runs and it is in use, its
 * times, when the owner was last opened and its project changed, what it is
 * related to (`related`, from the lens), and AI review's note. Only what the
 * API reports; a question nothing answered is Unknown, never "no" (ADR-0053).
 */
export function evidenceStack(
  candidate: CandidateView,
  related: readonly LensLine[],
): EvidenceLine[] {
  const review =
    candidate.verdict === "review"
      ? [...new Set(candidate.review_reasons?.codes ?? [])].filter(
          (code) =>
            !["active_process", "possible_user_data", "tool_required", "ai_caution"].includes(code),
        )
      : [];
  const lines: EvidenceLine[] = candidate.reasons
    .filter((reason) => reason.rule !== "default.review" || review.length === 0)
    .map((reason) => ({
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
  for (const code of review) {
    lines.push({ type: "review_reason", code, source: "rule", certainty: "unknown" });
  }
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
    if (fact.type === "application_running") {
      lines.push({
        type: "app_running",
        app: fact.app,
        running: fact.running,
        source: "process",
        certainty: "resolved",
      });
    } else if (fact.type === "in_use") {
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

  // Usage dates are shown, never decided on, and say only what the platform
  // recorded: not every launch, and only the project's marker files (ADR-0059).
  for (const fact of facts) {
    if (fact.type === "application_last_opened") {
      lines.push({
        type: "app_opened",
        app: fact.app,
        at: fact.at,
        source: "time",
        certainty: "partial",
      });
    } else if (fact.type === "project_activity") {
      lines.push({ type: "project_activity", at: fact.at, source: "time", certainty: "partial" });
    }
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
