import type { CandidateView } from "@lumen/api-client";
import { useId, useRef } from "react";
import { type Certainty, CertaintyDot, statusOf } from "@/entities/scan-session";
import { useI18n } from "@/shared/i18n";
import { baseName } from "@/shared/lib";
import { CloseIcon, Dialog } from "@/shared/ui";
import { lineText } from "../lib/evidence";
import type { Lens } from "../model/lens";
import { evidenceStack } from "../model/stack";
import { CandidateSize } from "./CandidateSize";
import { CheckAgain } from "./CheckAgain";
import { StatusText } from "./Status";
import { ToolCommand } from "./ToolCommand";

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
              {/* The answer first; why, and what it means, follow below. */}
              <p className="mt-1.5 text-[13px]">
                <StatusText status={status} />
              </p>
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
                <CandidateSize candidate={candidate} />
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
                    <span className="text-[12px] leading-[20px] font-medium tracking-[0.04em] text-ink-3 uppercase">
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
            <CheckAgain key={candidate.path} candidate={candidate} />
            {candidate.tool_command !== null && <ToolCommand command={candidate.tool_command} />}
          </div>
        </div>
      )}
    </Dialog>
  );
}
