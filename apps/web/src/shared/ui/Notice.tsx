import type { ReactNode } from "react";
import { CheckIcon, WarningIcon } from "./icons";

export type NoticeTone = "warning" | "success";

function toneClass(tone: NoticeTone): string {
  switch (tone) {
    case "warning":
      return "border-amber-200 bg-amber-50 text-amber-950 dark:border-amber-800/60 dark:bg-amber-950/40 dark:text-amber-100";
    case "success":
      return "border-emerald-200 bg-emerald-50 text-emerald-950 dark:border-emerald-800/60 dark:bg-emerald-950/40 dark:text-emerald-100";
  }
}

function ToneIcon({ tone }: { readonly tone: NoticeTone }) {
  switch (tone) {
    case "warning":
      return <WarningIcon className="mt-0.5 text-amber-600 dark:text-amber-400" />;
    case "success":
      return <CheckIcon className="mt-0.5 text-emerald-600 dark:text-emerald-400" />;
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
    <div className={`flex gap-3 rounded-xl border p-4 text-sm ${toneClass(tone)}`}>
      <ToneIcon tone={tone} />
      <div className="min-w-0 flex-1 space-y-1">{children}</div>
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
        <p className="font-mono text-xs break-all text-amber-900/80 dark:text-amber-200/80">
          {String(error)}
        </p>
      </Notice>
    </div>
  );
}
