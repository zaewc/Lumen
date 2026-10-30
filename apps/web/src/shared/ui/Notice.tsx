import type { ReactNode } from "react";
import { useI18n } from "@/shared/i18n";
import { errorMessage } from "@/shared/lib";
import { Disclosure } from "./Disclosure";
import { CheckIcon, WarningIcon } from "./icons";

export type NoticeTone = "warning" | "success";

function ToneIcon({ tone }: { readonly tone: NoticeTone }) {
  switch (tone) {
    case "warning":
      return <WarningIcon className="mt-px size-4 text-amber-600 dark:text-amber-400" />;
    case "success":
      return <CheckIcon className="mt-px size-4 text-emerald-600 dark:text-emerald-400" />;
  }
}

/**
 * A short, calm message on one line (or a few): a coloured glyph says what
 * kind it is, the text stays in the ordinary ink. No box around it.
 */
export function Notice({
  tone,
  children,
}: {
  readonly tone: NoticeTone;
  readonly children: ReactNode;
}) {
  return (
    <div className="flex gap-2 text-[13px] leading-relaxed text-ink">
      <ToneIcon tone={tone} />
      <div className="min-w-0 flex-1 space-y-1">{children}</div>
    </div>
  );
}

/**
 * Something did not work: what happened, then (as `children`) how to
 * recover, such as a "Try again" button. The core's own error is under
 * "Details", for a report rather than for reading.
 */
export function ErrorNotice({
  message,
  error,
  children,
}: {
  readonly message: string;
  readonly error: unknown;
  readonly children?: ReactNode;
}) {
  return (
    <div role="alert">
      <Notice tone="warning">
        <p className="font-medium">{message}</p>
        {children !== undefined && <div className="flex flex-wrap gap-2 pt-1">{children}</div>}
        <ErrorDetails error={error} />
      </Notice>
    </div>
  );
}

/**
 * "Details" with the core's own words about `error` (the host's message or
 * an `Error`'s message), or a generic line when it carries none.
 */
export function ErrorDetails({ error }: { readonly error: unknown }) {
  const { m } = useI18n();
  return (
    <Disclosure summary={<span className="text-[12px] text-ink-3">{m.common.details}</span>}>
      <p className="font-mono text-[11px] break-all text-ink-3 select-text">
        {errorMessage(error) ?? m.common.noDetails}
      </p>
    </Disclosure>
  );
}
