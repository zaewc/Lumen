import type { ReactNode } from "react";

/** The width the page's toolbar and content share, so their edges line up. */
const COLUMN = "mx-auto w-full max-w-[1248px] px-5 min-[720px]:px-10";

/**
 * A readable page title and its actions, aligned with the content below.
 * Actions stay available while the artifact list scrolls.
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
  return (
    <main className="@container flex min-h-full flex-col">
      <header className="sticky top-0 z-10 border-b border-line bg-window">
        <div className={`${COLUMN} flex min-h-24 flex-wrap items-center gap-x-4 gap-y-3 py-5`}>
          <h1 className="text-[28px] leading-tight font-semibold tracking-[-0.045em] text-ink">
            {title}
          </h1>
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
