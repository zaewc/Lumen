import type { ReactNode } from "react";

/** A rounded box on the window background, with a hairline edge. */
export function Card({
  children,
  className = "",
}: {
  readonly children: ReactNode;
  readonly className?: string;
}) {
  return (
    <section className={`rounded-xl border border-line bg-surface p-5 ${className}`}>
      {children}
    </section>
  );
}
