import { useId } from "react";

/** One choice of a segmented control. */
export interface Segment<T extends string> {
  readonly value: T;
  readonly label: string;
  /** A count shown after the label, if any. */
  readonly count?: number;
}

/**
 * A precise choice row with an underline: native radio buttons underneath, so the
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
      className={`${stretch ? "flex w-full" : "inline-flex max-w-full flex-wrap"} relative min-w-0 gap-1 border-b border-line`}
    >
      <legend className="sr-only">{label}</legend>
      {segments.map((segment) => (
        <label
          key={segment.value}
          className={`${stretch ? "flex-1" : ""} relative -mb-px flex min-h-9 shrink-0 cursor-pointer items-center justify-center gap-1.5 border-b-2 border-transparent px-3 text-[13px] font-medium whitespace-nowrap text-ink-2 transition-colors select-none hover:bg-fill hover:text-ink has-checked:border-accent has-checked:text-accent-ink has-focus-visible:outline-2 has-focus-visible:-outline-offset-2 has-focus-visible:outline-accent motion-reduce:transition-none`}
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
