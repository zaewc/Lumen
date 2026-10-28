import { useId } from "react";

/** One choice of a segmented control. */
export interface Segment<T extends string> {
  readonly value: T;
  readonly label: string;
  /** A count shown after the label, if any. */
  readonly count?: number;
}

/**
 * A segmented control, as macOS draws one: radio buttons underneath, so the
 * arrow keys move between choices and screen readers announce a group.
 * Counts are left out when the page is narrow (below 28rem), and segments
 * that still do not fit wrap onto another line: the page never scrolls
 * sideways.
 */
export function Segmented<T extends string>({
  label,
  segments,
  value,
  onChange,
  stretch = false,
}: {
  /** What the control chooses, for screen readers. */
  readonly label: string;
  readonly segments: readonly Segment<T>[];
  readonly value: T;
  readonly onChange: (value: T) => void;
  /** Fill the width, sharing it equally. */
  readonly stretch?: boolean;
}) {
  const name = useId();
  return (
    <fieldset
      className={`${stretch ? "flex w-full" : "inline-flex max-w-full flex-wrap"} min-w-0 rounded-lg bg-fill-strong p-0.5`}
    >
      <legend className="sr-only">{label}</legend>
      {segments.map((segment) => (
        <label
          key={segment.value}
          className={`${stretch ? "flex-1" : ""} relative flex shrink-0 cursor-pointer items-center justify-center gap-1.5 rounded-md px-2.5 py-1 @md:px-3 text-[13px] font-medium whitespace-nowrap text-ink-2 transition-colors select-none hover:text-ink has-checked:bg-raised has-checked:text-ink has-checked:shadow-sm has-checked:shadow-black/10 has-focus-visible:outline-2 has-focus-visible:outline-accent motion-reduce:transition-none dark:has-checked:bg-white/15`}
        >
          <input
            type="radio"
            name={name}
            value={segment.value}
            checked={segment.value === value}
            onChange={() => onChange(segment.value)}
            className="sr-only"
          />
          <span>{segment.label}</span>
          {segment.count !== undefined && (
            <span className="sr-only text-[12px] font-normal tabular-nums text-ink-3 @md:not-sr-only">
              {segment.count}
            </span>
          )}
        </label>
      ))}
    </fieldset>
  );
}
