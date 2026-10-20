import type { ReactNode } from "react";
import { ChevronIcon } from "./icons";

/**
 * A native `<details>` disclosure (keyboard and screen-reader support built
 * in) with a chevron that turns when it is open. Closed unless `defaultOpen`.
 */
export function Disclosure({
  summary,
  children,
  defaultOpen = false,
  className = "",
}: {
  readonly summary: ReactNode;
  readonly children: ReactNode;
  readonly defaultOpen?: boolean;
  readonly className?: string;
}) {
  return (
    <details open={defaultOpen} className={className}>
      <summary className="-mx-2 flex cursor-pointer list-none items-center gap-2 rounded-lg px-2 py-1 select-none hover:bg-neutral-100 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-indigo-600 [&::-webkit-details-marker]:hidden">
        <ChevronIcon className="text-neutral-500 transition-transform motion-reduce:transition-none [details[open]>summary>&]:rotate-90" />
        {summary}
      </summary>
      <div className="mt-2">{children}</div>
    </details>
  );
}
