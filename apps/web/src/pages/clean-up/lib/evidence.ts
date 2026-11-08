import {
  aiNoteText,
  formatDateTime,
  type Locale,
  type Messages,
  reasonText,
  reviewReasonText,
} from "@/shared/i18n";
import { formatBytes } from "@/shared/lib";
import type { LensLine } from "../model/lens";
import type { EvidenceLine } from "../model/stack";

/** One relation of the open item's lens, in words. */
function relationText(m: Messages, line: LensLine): string {
  const size = formatBytes(line.bytes);
  switch (line.relation) {
    case "tool":
      return m.detail.lens.tool(line.count, line.tool, size);
    case "processes":
      return m.detail.lens.processes(line.count, size, ...line.processes);
    case "evidence":
      return m.detail.lens.evidence(line.count, size);
    case "kind":
      return m.detail.lens.kind(line.count, m.results.groups[line.kind], size);
  }
}

/** A time, saying only as much as the platform can vouch for. */
function timeText(m: Messages, locale: Locale, line: EvidenceLine & { type: "time" }): string {
  switch (line.time.state) {
    case "recorded":
      return m.detail.changed(formatDateTime(locale, line.time.at), line.folder);
    case "may_lag":
      return m.detail.opened(formatDateTime(locale, line.time.at), line.folder);
    case "not_recorded":
      return m.detail.notRecorded(line.which);
    case "not_read":
      return m.detail.notRead(line.which);
  }
}

/** One line of evidence, in words. */
export function lineText(m: Messages, locale: Locale, line: EvidenceLine): string {
  switch (line.type) {
    case "reason":
      return reasonText(m, line.reason);
    case "review_reason":
      return reviewReasonText(m, line.code);
    case "owner":
      return m.detail.owner(line.app, line.present);
    case "app_running":
      return m.detail.appRunning(line.app, line.running);
    case "in_use":
      return m.detail.inUse(line.inUse);
    case "unseen":
      return m.detail.unseen(line.unseen === null ? null : Number(line.unseen));
    case "time":
      return timeText(m, locale, line);
    case "app_opened":
      return m.detail.appOpened(line.app, formatDateTime(locale, line.at));
    case "project_activity":
      return m.detail.projectChanged(formatDateTime(locale, line.at));
    case "related":
      return relationText(m, line.line);
    case "unrelated":
      return m.detail.relatedNone;
    case "note":
      return aiNoteText(m, line.note);
  }
}
