import { useEffect, useRef } from "react";

/**
 * A setup step's title and what it is about. A step that replaces another
 * (`focus`) moves focus to its title, since the button just pressed is gone;
 * the first step takes no focus (ADR-0050).
 */
export function StepHeading({
  id,
  title,
  intro,
  focus = false,
}: {
  readonly id: string;
  readonly title: string;
  readonly intro: string;
  readonly focus?: boolean;
}) {
  const heading = useRef<HTMLHeadingElement>(null);
  useEffect(() => {
    if (focus) heading.current?.focus();
  }, [focus]);
  return (
    <>
      <h1 ref={heading} id={id} tabIndex={-1} className="mt-5 text-title text-ink outline-none">
        {title}
      </h1>
      <p className="mt-2 text-ink-2">{intro}</p>
    </>
  );
}
