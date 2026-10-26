import type { CandidateView } from "@lumen/api-client";
import { type ReactNode, useId, useRef } from "react";
import { statusOf } from "@/entities/scan-session";
import { aiNoteText, reasonText, useI18n } from "@/shared/i18n";
import { baseName, formatBytes } from "@/shared/lib";
import { CloseIcon, Dialog, SparkleIcon } from "@/shared/ui";
import { StatusText } from "./Status";
import { ToolCommand } from "./ToolCommand";

/** One labelled figure of the details, label and value side by side. */
function Field({ label, children }: { readonly label: string; readonly children: ReactNode }) {
  return (
    <>
      <dt className="text-ink-2">{label}</dt>
      <dd className="min-w-0 text-ink">{children}</dd>
    </>
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
          <header className="flex items-start gap-3 border-b border-line px-4 py-3">
            <div className="min-w-0 flex-1">
              <h2 id={title} className="text-[15px] font-semibold break-words text-ink">
                {baseName(candidate.path)}
              </h2>
              <p className="mt-0.5 text-[12px] text-ink-2">
                {m.results.groups[candidate.group.kind]}
                {candidate.group.source !== null && ` · ${candidate.group.source}`}
              </p>
            </div>
            <button
              ref={close}
              type="button"
              onClick={onClose}
              aria-label={m.common.close}
              title={`${m.common.close} (Esc)`}
              className="-mr-1 flex size-7 shrink-0 items-center justify-center rounded-md text-ink-2 hover:bg-fill-strong hover:text-ink focus-visible:outline-2 focus-visible:outline-accent"
            >
              <CloseIcon className="size-4" />
            </button>
          </header>
          <div className="flex-1 divide-y divide-line overflow-y-auto px-4 [&>*]:py-4">
            <dl className="grid grid-cols-[auto_minmax(0,1fr)] gap-x-4 gap-y-2">
              <Field label={m.detail.size}>
                <span className="font-semibold tabular-nums">
                  {formatBytes(candidate.reclaimable_bytes)}
                </span>
              </Field>
              <Field label={m.detail.decision}>
                <StatusText status={statusOf(candidate)} />
              </Field>
              <Field label={m.detail.location}>
                <code className="block font-mono text-[12px] leading-relaxed break-all select-all">
                  {candidate.path}
                </code>
              </Field>
            </dl>
            <section>
              <h3 className="text-[12px] font-semibold text-ink-2">{m.detail.reasons}</h3>
              {demoted && (
                <p className="mt-2 flex gap-2 leading-snug text-ink">
                  <SparkleIcon className="mt-px size-4 text-accent-ink" />
                  <span>
                    <span className="font-medium">{m.detail.aiTitle}</span> {m.detail.aiBody}
                  </span>
                </p>
              )}
              <ul className="mt-2 list-disc space-y-1.5 pl-5 leading-snug text-ink marker:text-ink-3">
                {candidate.reasons.map((reason) => (
                  <li key={reason.rule}>{reasonText(m, reason)}</li>
                ))}
              </ul>
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
