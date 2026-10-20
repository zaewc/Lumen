import type { ReactNode } from "react";

/** A rounded, lightly raised panel: the building block of every page. */
export function Card({
  children,
  className = "",
}: {
  readonly children: ReactNode;
  readonly className?: string;
}) {
  return (
    <section
      className={`rounded-2xl border border-neutral-200 bg-white p-5 shadow-sm sm:p-6 dark:border-neutral-800 dark:bg-neutral-900 dark:shadow-none ${className}`}
    >
      {children}
    </section>
  );
}
