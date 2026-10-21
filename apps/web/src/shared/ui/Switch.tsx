import type { Ref } from "react";

/**
 * An on/off switch, as macOS draws one: a real button with the `switch`
 * role, so Space and Enter toggle it and screen readers say "on" or "off".
 */
export function Switch({
  ref,
  checked,
  onChange,
  disabled = false,
  labelledBy,
  describedBy,
}: {
  readonly ref?: Ref<HTMLButtonElement>;
  readonly checked: boolean;
  readonly onChange: (next: boolean) => void;
  readonly disabled?: boolean;
  readonly labelledBy: string;
  readonly describedBy?: string;
}) {
  return (
    <button
      ref={ref}
      type="button"
      role="switch"
      aria-checked={checked}
      aria-labelledby={labelledBy}
      aria-describedby={describedBy}
      disabled={disabled}
      onClick={() => onChange(!checked)}
      className="group relative inline-flex h-[22px] w-[38px] shrink-0 items-center rounded-full bg-black/15 p-0.5 transition-colors focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent disabled:cursor-not-allowed disabled:opacity-50 aria-checked:bg-accent motion-reduce:transition-none dark:bg-white/20 dark:aria-checked:bg-accent"
    >
      <span className="size-[18px] rounded-full bg-white shadow-sm shadow-black/30 transition-transform group-aria-checked:translate-x-4 motion-reduce:transition-none" />
    </button>
  );
}
