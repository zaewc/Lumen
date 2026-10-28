import { type FormEvent, useId } from "react";
import { isOn, useAiSettings } from "@/entities/ai-review";
import { useI18n } from "@/shared/i18n";
import { buttonClass, Disclosure, ErrorNotice } from "@/shared/ui";
import { rootsOf } from "../lib/roots";
import { StepHeading } from "./StepHeading";

/**
 * Before a scan: what to do (the toolbar's "Scan disk"), and folders for
 * those who want to scan only some.
 */
export function StartPanel({
  folders,
  onFoldersChange,
  onScan,
  error,
  focus,
}: {
  readonly folders: string;
  readonly onFoldersChange: (text: string) => void;
  /** Scans these roots. */
  readonly onScan: (roots: string[]) => void;
  readonly error: unknown;
  readonly focus: boolean;
}) {
  const { m } = useI18n();
  const field = useId();
  const ai = useAiSettings();
  const aiInUse = ai.isSuccess ? isOn(ai.data) : null;
  const roots = rootsOf(folders);
  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (roots.length > 0) {
      onScan(roots);
    }
  };
  return (
    <section className="max-w-xl space-y-3">
      <div className="space-y-1">
        <StepHeading focus={focus} className="text-[15px] font-semibold text-ink">
          {m.start.title}
        </StepHeading>
        <p className="text-ink-2">{m.start.promise}</p>
        <p className="text-[12px] text-ink-3">{m.start.wholeDiskHint}</p>
        {aiInUse !== null && (
          <p className="text-[12px] text-ink-3">{m.start.aiOn(aiInUse.service)}</p>
        )}
      </div>
      {error !== null && <ErrorNotice message={m.start.failed} error={error} />}
      <Disclosure summary={<span className="text-ink-2">{m.start.chooseFolders}</span>}>
        <form onSubmit={submit} className="space-y-2">
          <label htmlFor={field} className="block text-[12px] text-ink-2">
            {m.start.foldersLabel}
          </label>
          <textarea
            id={field}
            value={folders}
            onChange={(event) => onFoldersChange(event.target.value)}
            rows={3}
            spellCheck={false}
            placeholder="/Users/you/Library/Developer"
            className="w-full rounded-md border border-line-strong bg-window p-2 font-mono text-[12px] text-ink placeholder:text-ink-3 focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-accent"
          />
          <button
            type="submit"
            disabled={roots.length === 0}
            title={roots.length === 0 ? m.start.foldersEmpty : undefined}
            className={buttonClass("secondary")}
          >
            {m.start.scanFolders}
          </button>
        </form>
      </Disclosure>
    </section>
  );
}
