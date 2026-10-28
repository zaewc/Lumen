import type { CandidateView } from "@lumen/api-client";
import { type ReactNode, useId, useRef } from "react";
import { statusOf } from "@/entities/scan-session";
import { aiNoteText, reasonText, useI18n } from "@/shared/i18n";
import { baseName, formatBytes } from "@/shared/lib";
import { CloseIcon, Dialog, SparkleIcon } from "@/shared/ui";
import { StatusText } from "./Status";
import { ToolCommand } from "./ToolCommand";

/** One step in the item's actual origin, location and decision trail. */
function Field({ label, children }: { readonly label: string; readonly children: ReactNode }) {
  return (
    <div className="relative border-l border-line-strong pb-5 pl-5 last:border-transparent last:pb-0 before:absolute before:top-1 before:-left-1 before:size-2 before:rounded-full before:border before:border-line-strong before:bg-raised">
      <dt className="text-[12px] text-ink-3">{label}</dt>
      <dd className="mt-1 min-w-0 text-[14px] text-ink">{children}</dd>
    </div>
  );
}

/**
 * Everything about one candidate, in a sheet along the window's right edge
 * (the whole window when it is narrow): what it frees, the verdict with
 * every reason, AI review's note on what an item to review likely is, where
 * it is, and the tool's own command if it has one.
 * Escape, the close button or a click beside it closes it.
 */
export function CandidateSheet({
  candidate,
  onClose,
}: {
  readonly candidate: CandidateView | null;
  readonly onClose: () => void;
}) {
  const { m } = useI18n();
  const title = useId();
  const close = useRef<HTMLButtonElement>(null);
  const demoted = candidate?.reasons.some((reason) => reason.rule === "jev.demote") ?? false;
  // An AI note is only ever on an item to review (ADR-0041).
  const note = candidate !== null && statusOf(candidate) === "review" ? candidate.ai_note : null;
  return (
    <Dialog
      open={candidate !== null}
      onClose={onClose}
      labelledBy={title}
      initialFocus={close}
      variant="sheet"
    >
      {candidate !== null && (
        <div className="flex h-full flex-col">
          <header className="flex items-start gap-4 border-b border-line px-6 py-5">
            <div className="min-w-0 flex-1">
              <p className="mb-1 text-[12px] text-ink-2">
                {m.results.groups[candidate.group.kind]}
              </p>
              <h2
                id={title}
                className="text-[22px] leading-snug font-semibold tracking-[-0.035em] break-words text-ink"
              >
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
              <p className="mt-1 text-[36px] leading-tight font-medium tracking-[-0.05em] text-ink tabular-nums">
                {formatBytes(candidate.reclaimable_bytes)}
              </p>
            </div>
            <dl aria-label={m.workspace.trail} className="pl-1">
              <Field label={m.workspace.origin}>
                {candidate.group.source ?? (
                  <span className="text-ink-3">{m.workspace.unknownOrigin}</span>
                )}
              </Field>
              <Field label={m.detail.location}>
                <code className="block font-mono text-[12px] leading-relaxed break-all select-all">
                  {candidate.path}
                </code>
              </Field>
              <Field label={m.detail.decision}>
                <StatusText status={statusOf(candidate)} />
              </Field>
            </dl>
            <section>
              <h3 className="text-[14px] font-semibold text-ink">{m.detail.reasons}</h3>
              {demoted && (
                <p className="mt-2 flex gap-2 leading-snug text-ink">
                  <SparkleIcon className="mt-px size-4 text-ink-3" />
                  <span>
                    <span className="font-medium">{m.detail.aiTitle}</span> {m.detail.aiBody}
                  </span>
                </p>
              )}
              <ol className="mt-2 divide-y divide-line text-[14px] leading-relaxed text-ink">
                {candidate.reasons.map((reason, index) => (
                  <li
                    key={reason.rule}
                    className="grid grid-cols-[1.5rem_minmax(0,1fr)] gap-3 py-3"
                  >
                    <span aria-hidden="true" className="pt-0.5 font-mono text-[11px] text-ink-3">
                      {String(index + 1).padStart(2, "0")}
                    </span>
                    <span>{reasonText(m, reason)}</span>
                  </li>
                ))}
              </ol>
              {note !== null && (
                <p className="mt-3 flex gap-2 leading-snug text-ink-2">
                  <SparkleIcon className="mt-px size-4 shrink-0 text-ink-3" />
                  <span>
                    {aiNoteText(m, note)}
                    <span className="block text-[12px] text-ink-3">{m.detail.aiNoteHint}</span>
                  </span>
                </p>
              )}
            </section>
            {candidate.tool_command !== null && <ToolCommand command={candidate.tool_command} />}
          </div>
        </div>
      )}
    </Dialog>
  );
}
