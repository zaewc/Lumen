import { type FormEvent, useId } from "react";
import { useI18n } from "@/shared/i18n";
import { buttonClass, Disclosure, ErrorNotice, InfoTip, SearchIcon } from "@/shared/ui";
import { rootsOf } from "../lib/roots";
import { StepHeading } from "./StepHeading";

/** Before a scan: one big button, and folders for those who want them. */
export function StartPanel({
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
    <section className="rounded-xl border border-line bg-surface px-6 py-10 min-[720px]:px-10 min-[720px]:py-12">
      <div className="flex flex-col items-center text-center">
        <span className="flex size-14 items-center justify-center rounded-2xl bg-accent-soft text-accent-ink">
          <SearchIcon className="size-7" />
        </span>
        <div className="mt-5 flex items-center gap-2">
          <StepHeading focus={focus} className="text-[20px] font-bold tracking-tight text-ink">
            {m.start.title}
          </StepHeading>
          <InfoTip term="scan" />
        </div>
        <p className="mt-2 max-w-md text-[13px] leading-relaxed text-ink-2">{m.start.promise}</p>
        {error !== null && (
          <div className="mt-5 w-full max-w-md text-left">
            <ErrorNotice message={m.start.failed} error={error} />
          </div>
        )}
        <button
          type="button"
          onClick={() => onScan([])}
          aria-describedby={hint}
          className={`${buttonClass("primary", "lg")} mt-7`}
        >
          {m.start.wholeDisk}
        </button>
        <p id={hint} className="mt-3 max-w-md text-[12px] leading-relaxed text-ink-3">
          {m.start.wholeDiskHint}
        </p>
      </div>
      <Disclosure
        className="mx-auto mt-8 max-w-md border-t border-line pt-4"
        summary={
          <span className="text-[13px] font-medium text-ink-2">{m.start.chooseFolders}</span>
        }
      >
        <form onSubmit={submit} className="space-y-3">
          <label htmlFor={field} className="block text-[13px] font-medium text-ink-2">
            {m.start.foldersLabel}
          </label>
          <textarea
            id={field}
            value={folders}
            onChange={(event) => onFoldersChange(event.target.value)}
            rows={3}
            spellCheck={false}
            placeholder="/Users/you/Library/Developer"
            className="w-full rounded-lg border border-line-strong bg-window p-2.5 font-mono text-[12px] text-ink placeholder:text-ink-3 focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-accent"
          />
          <button type="submit" disabled={roots.length === 0} className={buttonClass("secondary")}>
            {m.start.scanFolders}
          </button>
        </form>
      </Disclosure>
    </section>
  );
}
