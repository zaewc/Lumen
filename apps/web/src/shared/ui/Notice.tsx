import type { ReactNode } from "react";
import { CheckIcon, WarningIcon } from "./icons";

export type NoticeTone = "warning" | "success";

function toneClass(tone: NoticeTone): string {
  switch (tone) {
    case "warning":
      return "border-amber-500/25 bg-amber-500/10 text-amber-950 dark:border-amber-400/20 dark:bg-amber-400/10 dark:text-amber-100";
    case "success":
      return "border-emerald-500/25 bg-emerald-500/10 text-emerald-950 dark:border-emerald-400/20 dark:bg-emerald-400/10 dark:text-emerald-100";
  }
}

function ToneIcon({ tone }: { readonly tone: NoticeTone }) {
  switch (tone) {
    case "warning":
      return <WarningIcon className="mt-px size-[18px] text-amber-600 dark:text-amber-400" />;
    case "success":
      return <CheckIcon className="mt-px size-[18px] text-emerald-600 dark:text-emerald-400" />;
  }
}

/** A short, calm message in a tinted box: amber for warnings. */
export function Notice({
  tone,
  children,
}: {
  readonly tone: NoticeTone;
  readonly children: ReactNode;
}) {
  return (
    <div className={`flex gap-2.5 rounded-xl border px-3.5 py-3 text-[13px] ${toneClass(tone)}`}>
      <ToneIcon tone={tone} />
      <div className="min-w-0 flex-1 space-y-1 leading-snug">{children}</div>
    </div>
  );
}

/**
 * Something did not work: a friendly sentence first, the core's own error
 * underneath in small print (for a report, not for reading).
 */
export function ErrorNotice({
  message,
  error,
}: {
  readonly message: string;
  readonly error: unknown;
}) {
  return (
    <div role="alert">
      <Notice tone="warning">
        <p className="font-medium">{message}</p>
        <p className="font-mono text-[11px] break-all text-amber-900/80 select-text dark:text-amber-200/80">
          {String(error)}
        </p>
      </Notice>
    </div>
  );
}
