/** How prominent an action is; red is kept for what cannot be undone. */
export type ButtonVariant = "primary" | "secondary" | "quiet" | "danger" | "dangerQuiet";

export type ButtonSize = "sm" | "md" | "lg";

const BASE =
  "inline-flex items-center justify-center gap-2 rounded-md font-medium whitespace-nowrap transition-[background-color,color,transform] duration-150 select-none focus-visible:outline-2 focus-visible:outline-offset-2 enabled:active:translate-y-px disabled:cursor-not-allowed disabled:opacity-45 motion-reduce:transform-none motion-reduce:transition-none";

function variantClass(variant: ButtonVariant): string {
  switch (variant) {
    case "primary":
      return "bg-accent text-white hover:bg-accent-hover focus-visible:outline-accent disabled:hover:bg-accent";
    case "secondary":
      return "border border-line-strong bg-raised text-ink hover:bg-fill focus-visible:outline-accent disabled:hover:bg-raised";
    case "quiet":
      return "text-accent-ink hover:bg-accent-soft focus-visible:outline-accent";
    case "danger":
      return "bg-red-600 text-white hover:bg-red-700 focus-visible:outline-red-600 disabled:hover:bg-red-600 dark:focus-visible:outline-red-400";
    case "dangerQuiet":
      return "text-red-700 hover:bg-red-600/10 focus-visible:outline-red-600 dark:text-red-400 dark:focus-visible:outline-red-400";
  }
}

function sizeClass(size: ButtonSize): string {
  switch (size) {
    case "sm":
      return "h-8 px-3 text-[13px]";
    case "md":
      return "h-9 px-3.5 text-[14px]";
    case "lg":
      return "h-10 px-4 text-[14px]";
  }
}

/** The classes of a button (or a link styled as one). */
export function buttonClass(variant: ButtonVariant, size: ButtonSize = "md"): string {
  return `${BASE} ${variantClass(variant)} ${sizeClass(size)}`;
}
