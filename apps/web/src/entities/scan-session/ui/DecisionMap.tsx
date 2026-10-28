import { useId } from "react";
import { useI18n } from "@/shared/i18n";
import { formatBytes } from "@/shared/lib";
import type { CandidateSummary, Status } from "../model/summary";
import { DecisionMark } from "./DecisionMark";

const ORDER: readonly Status[] = ["keep", "review", "command", "quarantine"];

/** A single composition of assessed candidates, never a claim about the whole disk. */
export function DecisionMap({
  summary,
  selected,
  onSelect,
}: {
  readonly summary: CandidateSummary;
  readonly selected: Status | null;
  readonly onSelect: (status: Status) => void;
}) {
  const { m } = useI18n();
  const name = useId();
  const total = ORDER.reduce((sum, status) => sum + BigInt(summary[status].bytes), 0n);
  return (
    <fieldset className="min-w-0">
      <legend className="mb-3 text-[12px] font-medium text-ink-2">{m.workspace.assessed}</legend>
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
      <div className="grid grid-cols-2 gap-x-5 min-[640px]:grid-cols-4">
        {ORDER.map((status) => (
          <label
            key={status}
            className="relative cursor-pointer border-b-2 border-transparent py-3 transition-colors hover:border-line-strong has-checked:border-accent has-focus-visible:outline-2 has-focus-visible:outline-accent motion-reduce:transition-none"
          >
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
              <span className="text-[21px] leading-tight font-medium tracking-[-0.04em] text-ink tabular-nums">
                {formatBytes(summary[status].bytes)}
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
