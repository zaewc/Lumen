import type { ReactNode } from "react";

/**
 * A page in the main area: its title (the page's only `h1`), an optional
 * line under it and actions on the right, then its content, at most 880 px
 * wide. The main area scrolls; `footer` stays at its bottom edge.
 */
export function Page({
  title,
  subtitle,
  actions,
  children,
  footer,
}: {
  readonly title: string;
  readonly subtitle?: ReactNode;
  readonly actions?: ReactNode;
  readonly children: ReactNode;
  readonly footer?: ReactNode;
}) {
  return (
    <main className="@container flex min-h-full flex-col">
      <div className="mx-auto w-full max-w-[880px] flex-1 px-4 pt-6 pb-10 min-[720px]:px-10 min-[720px]:pt-9">
        <header className="mb-6 flex flex-wrap items-end justify-between gap-x-4 gap-y-2">
          <div className="min-w-0">
            <h1 className="text-[22px] leading-tight font-bold tracking-tight text-ink">{title}</h1>
            {subtitle !== undefined && (
              <div className="mt-1 text-[13px] text-ink-2">{subtitle}</div>
            )}
          </div>
          {actions !== undefined && <div className="flex shrink-0 gap-2">{actions}</div>}
        </header>
        <div className="space-y-5">{children}</div>
      </div>
      {footer}
    </main>
  );
}
