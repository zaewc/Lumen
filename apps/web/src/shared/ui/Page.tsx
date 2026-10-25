import type { ReactNode } from "react";

/** The width the page's toolbar and content share, so their edges line up. */
const COLUMN = "mx-auto w-full max-w-[1040px] px-4 min-[720px]:px-6";

/**
 * A page in the main area, as a macOS window lays one out: a slim toolbar
 * with its title (the page's only `h1`), anything beside it, and its actions
 * on the right; then the content. The toolbar stays at the top while the
 * main area scrolls; `footer` stays at its bottom edge.
 */
export function Page({
  title,
  accessory,
  actions,
  children,
  footer,
}: {
  readonly title: string;
  /** Beside the title: a glossary tip or a short figure. */
  readonly accessory?: ReactNode;
  readonly actions?: ReactNode;
  readonly children: ReactNode;
  readonly footer?: ReactNode;
}) {
  return (
    <main className="@container flex min-h-full flex-col">
      <header className="sticky top-0 z-10 border-b border-line bg-window">
        <div className={`${COLUMN} flex min-h-12 flex-wrap items-center gap-x-3 gap-y-1 py-2`}>
          <h1 className="text-[15px] font-semibold text-ink">{title}</h1>
          {accessory !== undefined && (
            <div className="flex min-w-0 items-center gap-2 text-[13px] text-ink-2">
              {accessory}
            </div>
          )}
          {actions !== undefined && (
            <div className="ml-auto flex shrink-0 items-center gap-2">{actions}</div>
          )}
        </div>
      </header>
      <div className={`${COLUMN} flex-1 space-y-6 pt-5 pb-10`}>{children}</div>
      {footer}
    </main>
  );
}
