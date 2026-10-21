import type { CandidateView } from "@lumen/api-client";
import { type ReactNode, useId, useRef } from "react";
import { reasonText, useI18n } from "@/shared/i18n";
import { baseName, formatBytes } from "@/shared/lib";
import { CloseIcon, Dialog, SparkleIcon } from "@/shared/ui";
import { KindIcon, VerdictBadge } from "./badges";
import { ToolCommand } from "./ToolCommand";

/** One labelled part of the details. */
function Field({ label, children }: { readonly label: string; readonly children: ReactNode }) {
  return (
    <div className="px-4 py-3">
      <dt className="text-[12px] font-medium text-ink-3">{label}</dt>
      <dd className="mt-1 text-[13px] text-ink">{children}</dd>
    </div>
  );
}

/**
 * Everything about one candidate, in a sheet along the window's right edge
 * (the whole window when it is narrow): where it is, what it frees, the
 * verdict with every reason, and the tool's own command if it has one.
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
          <header className="flex items-start gap-3 border-b border-line px-4 py-4">
            <KindIcon kind={candidate.group.kind} />
            <div className="min-w-0 flex-1">
              <h2 id={title} className="text-[16px] font-semibold break-words text-ink">
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
              className="-mt-1 -mr-1 flex size-8 shrink-0 items-center justify-center rounded-full text-ink-2 hover:bg-fill-strong hover:text-ink focus-visible:outline-2 focus-visible:outline-accent"
            >
              <CloseIcon className="size-[18px]" />
            </button>
          </header>
          <div className="flex-1 space-y-4 overflow-y-auto bg-window px-4 py-4">
            <dl className="divide-y divide-line rounded-xl border border-line bg-surface">
              <Field label={m.detail.size}>
                <span className="text-[20px] font-semibold tabular-nums">
                  {formatBytes(candidate.reclaimable_bytes)}
                </span>
              </Field>
              <Field label={m.detail.decision}>
                <VerdictBadge verdict={candidate.verdict} />
              </Field>
              <Field label={m.detail.location}>
                <code className="block font-mono text-[12px] leading-relaxed break-all text-ink select-all">
                  {candidate.path}
                </code>
              </Field>
            </dl>
            {demoted && (
              <div className="flex gap-3 rounded-xl border border-accent/25 bg-accent-soft px-4 py-3">
                <SparkleIcon className="mt-0.5 size-[18px] text-accent-ink" />
                <div className="text-[13px]">
                  <p className="font-semibold text-ink">{m.detail.aiTitle}</p>
                  <p className="mt-0.5 leading-snug text-ink-2">{m.detail.aiBody}</p>
                </div>
              </div>
            )}
            <section className="rounded-xl border border-line bg-surface px-4 py-3">
              <h3 className="text-[12px] font-medium text-ink-3">{m.detail.reasons}</h3>
              <ul className="mt-2 space-y-2 text-[13px] leading-snug text-ink">
                {candidate.reasons.map((reason) => (
                  <li key={reason.rule} className="flex gap-2">
                    <span
                      aria-hidden="true"
                      className="mt-[7px] size-1.5 shrink-0 rounded-full bg-ink-3"
                    />
                    <span>{reasonText(m, reason)}</span>
                  </li>
                ))}
              </ul>
            </section>
            {candidate.tool_command !== null && <ToolCommand command={candidate.tool_command} />}
          </div>
        </div>
      )}
    </Dialog>
  );
}
