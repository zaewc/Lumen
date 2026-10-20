import { type FormEvent, useId } from "react";
import { useI18n } from "@/shared/i18n";
import { buttonClass, Card, Disclosure, ErrorNotice, InfoTip } from "@/shared/ui";
import { rootsOf } from "../lib/roots";
import { StepHeading } from "./StepHeading";

/** The first step: one big button, and folders for those who want them. */
export function StartCard({
  folders,
  onFoldersChange,
  onScan,
  error,
  focus,
}: {
  readonly folders: string;
  readonly onFoldersChange: (text: string) => void;
  /** Scans these roots; none means the whole startup disk. */
  readonly onScan: (roots: string[]) => void;
  readonly error: unknown;
  readonly focus: boolean;
}) {
  const { m } = useI18n();
  const hint = useId();
  const field = useId();
  const roots = rootsOf(folders);
  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (roots.length > 0) {
      onScan(roots);
    }
  };
  return (
    <Card className="sm:p-10">
      <StepHeading focus={focus} className="text-2xl font-semibold tracking-tight sm:text-3xl">
        {m.start.title}
      </StepHeading>
      <div className="mt-3 flex items-start gap-2 text-neutral-600">
        <p>{m.start.promise}</p>
        <InfoTip term="scan" />
      </div>
      {error !== null && (
        <div className="mt-6">
          <ErrorNotice message={m.start.failed} error={error} />
        </div>
      )}
      <button
        type="button"
        onClick={() => onScan([])}
        aria-describedby={hint}
        className={`${buttonClass("primary", "lg")} mt-8 w-full sm:w-auto`}
      >
        {m.start.wholeDisk}
      </button>
      <p id={hint} className="mt-3 text-sm text-neutral-500">
        {m.start.wholeDiskHint}
      </p>
      <Disclosure
        className="mt-8 border-t border-neutral-100 pt-4"
        summary={
          <span className="text-sm font-medium text-neutral-700">{m.start.chooseFolders}</span>
        }
      >
        <form onSubmit={submit} className="space-y-3">
          <label htmlFor={field} className="block text-sm font-medium text-neutral-700">
            {m.start.foldersLabel}
          </label>
          <textarea
            id={field}
            value={folders}
            onChange={(event) => onFoldersChange(event.target.value)}
            rows={3}
            spellCheck={false}
            placeholder="/Users/you/Library/Developer"
            className="w-full rounded-xl border border-neutral-300 bg-white p-3 font-mono text-sm focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-indigo-600"
          />
          <button type="submit" disabled={roots.length === 0} className={buttonClass("secondary")}>
            {m.start.scanFolders}
          </button>
        </form>
      </Disclosure>
    </Card>
  );
}
