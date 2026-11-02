import { type ReactNode, useId } from "react";
import { useI18n } from "@/shared/i18n";
import { Bytes } from "@/shared/ui";
import type { CandidateSummary, Status } from "../model/summary";
import { DecisionMark } from "./DecisionMark";

const ORDER: readonly Status[] = ["keep", "review", "command", "quarantine"];

const CHOICE =
  "relative cursor-pointer border-b-2 border-transparent py-3 transition-colors hover:border-line-strong has-checked:border-accent has-focus-visible:outline-2 has-focus-visible:outline-accent motion-reduce:transition-none";

/**
 * A single composition of assessed candidates, never a claim about the whole
 * disk. Each status is a choice; with `withAll`, "All" comes first and is
 * chosen while `selected` is null, so the legend is the one status filter.
 */
export function DecisionMap({
  summary,
  selected,
  onSelect,
  withAll = false,
  accessory,
  track = true,
}: {
  readonly summary: CandidateSummary;
  readonly selected: Status | null;
  /** A status, or `null` for "All". */
  readonly onSelect: (status: Status | null) => void;
  /** Offer "All" as the first choice. */
  readonly withAll?: boolean;
  /** Beside the legend: a glossary tip. */
  readonly accessory?: ReactNode;
  /** Draw the composition as a bar; off where a map above already draws it. */
  readonly track?: boolean;
}) {
  const { m } = useI18n();
  const name = useId();
  const total = ORDER.reduce((sum, status) => sum + BigInt(summary[status].bytes), 0n);
  return (
    <fieldset className="min-w-0">
      <legend className="mb-3 flex items-center gap-2 text-[12px] font-medium text-ink-2">
        {m.workspace.assessed}
        {accessory}
      </legend>
      {track && (
        <div aria-hidden="true" className="decision-track">
          {ORDER.map((status) => (
            <span
              key={status}
              className="decision-segment"
              data-status={status}
              style={{
                width: `${total === 0n ? 0 : Number((BigInt(summary[status].bytes) * 1000n) / total) / 10}%`,
              }}
            />
          ))}
        </div>
      )}
      <div
        className={`grid grid-cols-2 gap-x-5 ${withAll ? "min-[640px]:grid-cols-5" : "min-[640px]:grid-cols-4"}`}
      >
        {withAll && (
          <label className={CHOICE}>
            <input
              type="radio"
              name={name}
              checked={selected === null}
              onChange={() => onSelect(null)}
              className="sr-only"
            />
            <span className="flex items-center gap-2 text-[13px] text-ink-2">
              {m.results.filters.all}
            </span>
            <span className="mt-1 flex flex-wrap items-baseline gap-x-2">
              <span className="text-figure font-medium text-ink tabular-nums">
                <Bytes value={total.toString()} />
              </span>
              <span className="text-[12px] text-ink-3">
                {m.common.items(ORDER.reduce((n, status) => n + summary[status].items.length, 0))}
              </span>
            </span>
          </label>
        )}
        {ORDER.map((status) => (
          <label key={status} className={CHOICE}>
            <input
              type="radio"
              name={name}
              checked={selected === status}
              onChange={() => onSelect(status)}
              className="sr-only"
            />
            <span className="flex items-center gap-2 text-[13px] text-ink-2">
              <DecisionMark status={status} />
              {m.results.status[status]}
            </span>
            <span className="mt-1 flex flex-wrap items-baseline gap-x-2">
              <span className="text-figure font-medium text-ink tabular-nums">
                <Bytes value={summary[status].bytes} />
              </span>
              <span className="text-[12px] text-ink-3">
                {m.common.items(summary[status].items.length)}
              </span>
            </span>
          </label>
        ))}
      </div>
    </fieldset>
  );
}
