import { type ReactNode, useEffect, useRef, useState } from "react";

/** The width the page's toolbar and content share, so their edges line up. */
const COLUMN = "mx-auto w-full max-w-[1248px] px-5 min-[720px]:px-10";

/**
 * A readable page title and its actions, aligned with the content below.
 * Actions stay available while the artifact list scrolls; the header's
 * hairline appears only once content scrolls under it.
 */
export function Page({
  title,
  accessory,
  actions,
  children,
}: {
  readonly title: string;
  /** Beside the title: a glossary tip or a short figure. */
  readonly accessory?: ReactNode;
  readonly actions?: ReactNode;
  readonly children: ReactNode;
}) {
  const top = useRef<HTMLDivElement>(null);
  const [scrolled, setScrolled] = useState(false);
  useEffect(() => {
    const element = top.current;
    if (element === null) {
      return;
    }
    // The line above the header leaves the scroll area once the page scrolls.
    const observer = new IntersectionObserver(([entry]) => {
      setScrolled(entry !== undefined && !entry.isIntersecting);
    });
    observer.observe(element);
    return () => observer.disconnect();
  }, []);
  return (
    <main className="@container flex min-h-full flex-col">
      <div ref={top} aria-hidden="true" className="-mb-px h-px" />
      <header
        data-scrolled={scrolled}
        className="sticky top-0 z-10 border-b border-transparent bg-canvas data-[scrolled=true]:border-line"
      >
        <div className={`${COLUMN} flex min-h-16 flex-wrap items-center gap-x-4 gap-y-3 py-3`}>
          <h1 className="text-title text-ink">{title}</h1>
          {accessory !== undefined && (
            <div className="flex min-w-0 items-center gap-2 text-[13px] text-ink-2">
              {accessory}
            </div>
          )}
          {actions !== undefined && (
            <div className="ml-auto flex max-w-full flex-wrap items-center justify-end gap-2">
              {actions}
            </div>
          )}
        </div>
      </header>
      <div className={`${COLUMN} flex-1 space-y-8 pt-7 pb-12`}>{children}</div>
    </main>
  );
}
