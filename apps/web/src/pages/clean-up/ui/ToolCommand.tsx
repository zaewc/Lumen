import type { ToolCommandView, ToolRunView } from "@lumen/api-client";
import { useMutation } from "@tanstack/react-query";
import { useEffect, useId, useState } from "react";
import { useTransport } from "@/shared/api";
import { type Messages, useI18n } from "@/shared/i18n";
import { buttonClass, ErrorNotice } from "@/shared/ui";
import { commandLine } from "../lib/command";

/** Whether the command was just copied, or copying failed. */
type CopyState = "idle" | "copied" | "failed";

/** How long "Copied" is shown. */
const COPIED_MS = 2000;

/** How a run ended, in words. */
function runText(m: Messages, run: ToolRunView): string {
  if (run.timed_out) {
    return m.toolCommand.timedOut;
  }
  return run.success ? m.toolCommand.succeeded : m.toolCommand.failed(run.exit_code);
}

/**
 * A developer tool's own cleanup command, shown in full. The user can copy
 * it, or, in the desktop app, run it after a confirmation that it cannot be
 * undone (ADR-0049). The host looks the command up by its entry; nothing
 * typed here reaches it.
 */
export function ToolCommand({ command }: { readonly command: ToolCommandView }) {
  const { m } = useI18n();
  const transport = useTransport();
  const warning = useId();
  const [copy, setCopy] = useState<CopyState>("idle");
  const [confirming, setConfirming] = useState(false);
  const line = commandLine(command);
  const runner = transport.runToolCommand;
  const run = useMutation({
    mutationFn: () => (runner ? runner(command.entry) : Promise.reject(new Error("not here"))),
    onSettled: () => setConfirming(false),
  });

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
    <section>
      <h3 className="text-[13px] font-semibold text-ink-2">{m.toolCommand.title(command.tool)}</h3>
      <div className="mt-2 flex items-start gap-2">
        <code className="min-w-0 flex-1 rounded-md bg-surface px-2.5 py-1.5 font-mono text-[12px] leading-relaxed break-all text-ink ring-1 ring-line select-all">
          {line}
        </code>
        <button
          type="button"
          className={`${buttonClass("secondary", "sm")} mt-0.5 shrink-0`}
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
      <p className="mt-2 text-[13px] leading-relaxed text-ink-2">
        {runner ? m.toolCommand.runNote : m.toolCommand.note}
      </p>
      {runner && !confirming && !run.isPending && (
        <button
          type="button"
          onClick={() => setConfirming(true)}
          className={`${buttonClass("secondary", "sm")} mt-2`}
        >
          {m.toolCommand.run} ›
        </button>
      )}
      {confirming && (
        <div className="mt-2 space-y-2 rounded-md bg-red-500/8 px-3 py-2.5 dark:bg-red-400/10">
          <div id={warning} className="space-y-1 text-[13px] text-red-900 dark:text-red-200">
            <p>{m.toolCommand.runWarning}</p>
            {command.removes_data && <p className="font-medium">{m.toolCommand.removesData}</p>}
          </div>
          <code className="block font-mono text-[12px] break-all text-ink">{line}</code>
          <div className="flex justify-end gap-2">
            <button
              type="button"
              onClick={() => setConfirming(false)}
              className={buttonClass("secondary", "sm")}
            >
              {m.common.close}
            </button>
            <button
              type="button"
              onClick={() => run.mutate()}
              disabled={run.isPending}
              aria-describedby={warning}
              className={buttonClass("danger", "sm")}
            >
              {m.toolCommand.runConfirm}
            </button>
          </div>
        </div>
      )}
      <div aria-live="polite" className="empty:hidden">
        {run.isPending && <p className="mt-2 text-[12px] text-ink-2">{m.toolCommand.running}</p>}
        {run.isSuccess && (
          <div className="mt-2 space-y-1">
            <p
              className={`text-[12px] ${run.data.success ? "text-emerald-700 dark:text-emerald-400" : "text-amber-800 dark:text-amber-300"}`}
            >
              {runText(m, run.data)}
            </p>
            {run.data.output.trim() !== "" && (
              <details className="text-[12px] text-ink-2">
                <summary className="cursor-pointer">{m.toolCommand.output}</summary>
                <pre className="mt-1 max-h-48 overflow-auto rounded-md bg-surface p-2 font-mono text-[11px] whitespace-pre-wrap text-ink ring-1 ring-line select-text">
                  {run.data.output}
                </pre>
              </details>
            )}
          </div>
        )}
      </div>
      {run.isError && (
        <div className="mt-2">
          <ErrorNotice message={m.toolCommand.runFailed} error={run.error} />
        </div>
      )}
    </section>
  );
}
