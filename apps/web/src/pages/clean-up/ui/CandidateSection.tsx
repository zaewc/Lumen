import type { CandidateView } from "@lumen/api-client";
import { reasonText, useI18n } from "@/shared/i18n";
import { baseName, formatBytes } from "@/shared/lib";
import { Card, Disclosure } from "@/shared/ui";
import type { VerdictGroup } from "../model/summary";

/** One candidate: its name, what it frees, why, and where it is. */
function CandidateRow({ candidate }: { readonly candidate: CandidateView }) {
  const { m } = useI18n();
  const [first] = candidate.reasons;
  return (
    <li className="py-3">
      <div className="flex items-baseline justify-between gap-3">
        <p className="min-w-0 font-semibold break-words text-neutral-900 dark:text-neutral-100">
          {baseName(candidate.path)}
        </p>
        <span className="shrink-0 text-sm font-medium tabular-nums text-neutral-700 dark:text-neutral-300">
          {formatBytes(candidate.reclaimable_bytes)}
        </span>
      </div>
      {first && (
        <p className="mt-0.5 text-sm text-neutral-700 dark:text-neutral-300">
          {reasonText(m, first)}
        </p>
      )}
      <p className="mt-1 font-mono text-xs break-all text-neutral-500 dark:text-neutral-400">
        {candidate.path}
      </p>
      {candidate.reasons.length > 1 && (
        <Disclosure
          className="mt-1"
          summary={
            <span className="text-xs font-medium text-indigo-700 dark:text-indigo-300">
              {m.common.why}
            </span>
          }
        >
          <ul className="list-disc space-y-1 pl-5 text-sm text-neutral-700 dark:text-neutral-300">
            {candidate.reasons.map((reason) => (
              <li key={reason.rule}>{reasonText(m, reason)}</li>
            ))}
          </ul>
        </Disclosure>
      )}
    </li>
  );
}

/** The candidates with one verdict, behind a header with their count and size. */
export function CandidateSection({
  title,
  hint,
  group,
  defaultOpen = false,
}: {
  readonly title: string;
  readonly hint?: string;
  readonly group: VerdictGroup;
  readonly defaultOpen?: boolean;
}) {
  const { m } = useI18n();
  if (group.items.length === 0) {
    return null;
  }
  return (
    <Card className="py-4 sm:py-4">
      <Disclosure
        defaultOpen={defaultOpen}
        summary={
          <>
            <h3 className="min-w-0 flex-1 font-semibold">{title}</h3>
            <span className="shrink-0 text-sm tabular-nums text-neutral-500 dark:text-neutral-400">
              {m.results.section(group.items.length, formatBytes(group.bytes))}
            </span>
          </>
        }
      >
        {hint !== undefined && (
          <p className="text-sm text-neutral-600 dark:text-neutral-400">{hint}</p>
        )}
        <ul className="divide-y divide-neutral-100 dark:divide-neutral-800">
          {group.items.map((candidate) => (
            <CandidateRow key={candidate.path} candidate={candidate} />
          ))}
        </ul>
      </Disclosure>
    </Card>
  );
}
