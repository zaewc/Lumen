import type { CandidateView } from "@lumen/api-client";
import { useId, useRef } from "react";
import { type Certainty, CertaintyDot, statusOf } from "@/entities/scan-session";
import {
  aiNoteText,
  formatDateTime,
  type Locale,
  type Messages,
  reasonText,
  useI18n,
} from "@/shared/i18n";
import { baseName, formatBytes } from "@/shared/lib";
import { Bytes, CloseIcon, Dialog } from "@/shared/ui";
import type { Lens, LensLine } from "../model/lens";
import { type EvidenceLine, evidenceStack } from "../model/stack";
import { StatusText } from "./Status";
import { ToolCommand } from "./ToolCommand";

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
function lineText(m: Messages, locale: Locale, line: EvidenceLine): string {
  switch (line.type) {
    case "reason":
      return reasonText(m, line.reason);
    case "owner":
      return m.detail.owner(line.app, line.present);
    case "in_use":
      return m.detail.inUse(line.inUse);
    case "unseen":
      return m.detail.unseen(line.unseen === null ? null : Number(line.unseen));
    case "time":
      return timeText(m, locale, line);
    case "related":
      return relationText(m, line.line);
    case "unrelated":
      return m.detail.relatedNone;
    case "note":
      return aiNoteText(m, line.note);
  }
}

const INK: { readonly [C in Certainty]: string } = {
  resolved: "text-ink",
  partial: "text-ink-2",
  unknown: "text-ink-3",
};

/**
 * Everything about one candidate, in a sheet along the window's right edge
 * (the whole window when it is narrow), read top to bottom as an argument
 * (docs/design/visual-language.md, evidence stack): what it is, each line of
 * evidence with its source and certainty, the decision and what it means,
 * then what can be done. Escape, the close button or a click beside it
 * closes it.
 */
export function CandidateSheet({
  candidate,
  lens,
  onClose,
}: {
  readonly candidate: CandidateView | null;
  /** What the candidate is related to (the map and list draw it too). */
  readonly lens: Lens | null;
  readonly onClose: () => void;
}) {
  const { m, locale } = useI18n();
  const title = useId();
  const close = useRef<HTMLButtonElement>(null);
  const status = candidate === null ? null : statusOf(candidate);
  return (
    <Dialog
      open={candidate !== null}
      onClose={onClose}
      labelledBy={title}
      initialFocus={close}
      variant="sheet"
    >
      {candidate !== null && status !== null && (
        <div className="flex h-full flex-col">
          <header className="flex items-start gap-4 border-b border-line px-6 py-5">
            <div className="min-w-0 flex-1">
              <p className="mb-1 truncate text-[12px] text-ink-2">
                {m.results.groups[candidate.group.kind]}
                {" · "}
                {candidate.group.source ?? (
                  <span className="text-ink-3">{m.workspace.unknownOrigin}</span>
                )}
              </p>
              <h2 id={title} className="text-title break-words text-ink">
                {baseName(candidate.path)}
              </h2>
            </div>
            <button
              ref={close}
              type="button"
              onClick={onClose}
              aria-label={m.common.close}
              title={`${m.common.close} (Esc)`}
              className="-mr-1 flex size-8 shrink-0 items-center justify-center rounded-md text-ink-2 hover:bg-fill-strong hover:text-ink focus-visible:outline-2 focus-visible:outline-accent"
            >
              <CloseIcon className="size-4" />
            </button>
          </header>
          <div className="flex-1 divide-y divide-line overflow-y-auto px-6 [&>*]:py-6">
            <div>
              <p className="text-[12px] text-ink-3">{m.detail.size}</p>
              <p className="mt-1 text-measure text-ink tabular-nums">
                <Bytes value={candidate.reclaimable_bytes} />
              </p>
              <p className="sr-only">{m.detail.location}</p>
              <code className="mt-2 block font-mono text-[12px] leading-relaxed break-all text-ink-2 select-all">
                {candidate.path}
              </code>
            </div>
            <section>
              <h3 className="text-section text-ink">{m.detail.reasons}</h3>
              <ul className="mt-2 text-[14px] leading-snug">
                {evidenceStack(candidate, lens?.lines ?? []).map((line, index) => (
                  <li
                    key={`${line.type}-${index}`}
                    className="grid grid-cols-[6px_5.5rem_minmax(0,1fr)] items-start gap-x-3 py-2 leading-[20px]"
                  >
                    <span className="flex h-5 items-center">
                      <CertaintyDot certainty={line.certainty} />
                    </span>
                    <span className="text-[11px] leading-[20px] font-medium tracking-[0.06em] text-ink-3 uppercase">
                      {m.detail.sources[line.source]}
                    </span>
                    <span className={`tabular-nums ${INK[line.certainty]}`}>
                      {lineText(m, locale, line)}
                      {line.type === "note" && (
                        <span className="block text-[12px] text-ink-3">{m.detail.aiNoteHint}</span>
                      )}
                    </span>
                  </li>
                ))}
              </ul>
              <div className="mt-3 border-t border-ink-3 pt-4">
                <h3 className="sr-only">{m.detail.decision}</h3>
                <p className="text-[15px]">
                  <StatusText status={status} />
                </p>
                <p className="mt-1 leading-snug text-ink-2">{m.detail.means[status]}</p>
                {candidate.verdict === "keep" && candidate.free_when_closed !== null && (
                  <p className="mt-3 leading-snug text-ink">
                    {m.detail.whenClosed(...candidate.free_when_closed.processes)}
                  </p>
                )}
              </div>
            </section>
            {candidate.tool_command !== null && <ToolCommand command={candidate.tool_command} />}
          </div>
        </div>
      )}
    </Dialog>
  );
}
