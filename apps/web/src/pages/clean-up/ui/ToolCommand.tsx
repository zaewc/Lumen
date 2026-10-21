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
    <section className="rounded-xl border border-line bg-surface px-4 py-3">
      <h3 className="text-[13px] font-semibold text-ink">{m.toolCommand.title(command.tool)}</h3>
      <div className="mt-2 flex items-start gap-2">
        <code className="min-w-0 flex-1 rounded-lg bg-neutral-900 px-3 py-2 font-mono text-[12px] leading-relaxed break-all text-neutral-100 select-all dark:bg-black/50">
          {line}
        </code>
        <button
          type="button"
          className={`${buttonClass("secondary", "sm")} mt-1 shrink-0`}
          onClick={() => {
            void copyLine();
          }}
        >
          {copy === "copied" ? m.toolCommand.copied : m.toolCommand.copy}
        </button>
      </div>
      <p aria-live="polite" className="text-[12px] text-amber-800 empty:hidden dark:text-amber-300">
        {copy === "failed" && <span className="mt-1 block">{m.toolCommand.copyFailed}</span>}
        {copy === "copied" && <span className="sr-only">{m.toolCommand.copied}</span>}
      </p>
      <p className="mt-2 text-[12px] leading-snug text-ink-3">{m.toolCommand.note}</p>
    </section>
  );
}
