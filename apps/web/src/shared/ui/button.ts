/** How prominent an action is; red is kept for what cannot be undone. */
export type ButtonVariant = "primary" | "secondary" | "quiet" | "danger" | "dangerQuiet";

export type ButtonSize = "sm" | "md" | "lg";

const BASE =
  "inline-flex items-center justify-center gap-2 rounded-xl font-medium transition-colors focus-visible:outline-2 focus-visible:outline-offset-2 disabled:cursor-not-allowed disabled:opacity-50";

function variantClass(variant: ButtonVariant): string {
  switch (variant) {
    case "primary":
      return "bg-indigo-600 text-white shadow-sm hover:bg-indigo-700 focus-visible:outline-indigo-600 disabled:hover:bg-indigo-600";
    case "secondary":
      return "border border-neutral-300 bg-white text-neutral-800 shadow-sm hover:bg-neutral-50 focus-visible:outline-indigo-600";
    case "quiet":
      return "text-neutral-700 hover:bg-neutral-100 focus-visible:outline-indigo-600";
    case "danger":
      return "bg-red-600 text-white shadow-sm hover:bg-red-700 focus-visible:outline-red-600 disabled:hover:bg-red-600";
    case "dangerQuiet":
      return "text-red-700 hover:bg-red-50 focus-visible:outline-red-600";
  }
}

function sizeClass(size: ButtonSize): string {
  switch (size) {
    case "sm":
      return "px-3 py-1.5 text-sm";
    case "md":
      return "px-4 py-2 text-sm";
    case "lg":
      return "px-6 py-3 text-base";
  }
}

/** The classes of a button (or a link styled as one). */
export function buttonClass(variant: ButtonVariant, size: ButtonSize = "md"): string {
  return `${BASE} ${variantClass(variant)} ${sizeClass(size)}`;
}
