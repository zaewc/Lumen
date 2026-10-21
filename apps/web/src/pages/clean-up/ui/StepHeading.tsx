import { type ReactNode, useEffect, useRef } from "react";

/**
 * The heading of a step. When the step replaces the one the user acted in
 * (`focus`), it takes focus, so keyboard and screen-reader users land on
 * what changed instead of on the page's start.
 */
export function StepHeading({
  children,
  focus,
  className = "",
}: {
  readonly children: ReactNode;
  readonly focus: boolean;
  readonly className?: string;
}) {
  const heading = useRef<HTMLHeadingElement>(null);
  useEffect(() => {
    if (focus) {
      heading.current?.focus();
    }
  }, [focus]);
  return (
    <h2 ref={heading} tabIndex={-1} className={`outline-none ${className}`}>
      {children}
    </h2>
  );
}
