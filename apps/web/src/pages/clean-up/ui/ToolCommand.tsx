import type { ToolCommandView } from "@lumen/api-client";
import { useEffect, useState } from "react";
import { useI18n } from "@/shared/i18n";
import { buttonClass } from "@/shared/ui";
import { commandLine } from "../lib/command";

/** Whether the command was just copied, or copying failed. */
type CopyState = "idle" | "copied" | "failed";

/** How long "Copied" is shown. */
const COPIED_MS = 2000;

/**
 * A developer tool's own cleanup command, for the user to copy and run
 * themselves. Lumen only shows it; nothing here runs it.
 */
export function ToolCommand({ command }: { readonly command: ToolCommandView }) {
  const { m } = useI18n();
  const [copy, setCopy] = useState<CopyState>("idle");
  const line = commandLine(command);

  useEffect(() => {
    if (copy !== "copied") {
      return;
    }
    const timer = setTimeout(() => setCopy("idle"), COPIED_MS);
    return () => clearTimeout(timer);
  }, [copy]);

  const copyLine = async () => {
    try {
      // Absent outside a secure context: the TypeError is caught below.
      await navigator.clipboard.writeText(line);
      setCopy("copied");
    } catch {
      // The command stays on screen and selectable.
      setCopy("failed");
    }
  };

  return (
    <div className="mt-2 rounded-xl border border-neutral-200 bg-neutral-50 p-3 dark:border-neutral-700 dark:bg-neutral-900">
      <p className="text-sm font-medium text-neutral-800 dark:text-neutral-200">
        {m.toolCommand.title(command.tool)}
      </p>
      <div className="mt-2 flex items-start gap-2">
        <code className="min-w-0 flex-1 rounded-lg bg-white px-2 py-1.5 font-mono text-xs break-all text-neutral-900 select-all ring-1 ring-neutral-200 dark:bg-neutral-950 dark:text-neutral-100 dark:ring-neutral-700">
          {line}
        </code>
        <button
          type="button"
          className={`${buttonClass("secondary", "sm")} shrink-0`}
          onClick={() => {
            void copyLine();
          }}
        >
          {copy === "copied" ? m.toolCommand.copied : m.toolCommand.copy}
        </button>
      </div>
      <p aria-live="polite" className="text-xs text-amber-800 empty:hidden dark:text-amber-300">
        {copy === "failed" && <span className="mt-1 block">{m.toolCommand.copyFailed}</span>}
        {copy === "copied" && <span className="sr-only">{m.toolCommand.copied}</span>}
      </p>
      <p className="mt-2 text-xs text-neutral-500 dark:text-neutral-400">{m.toolCommand.note}</p>
    </div>
  );
}
