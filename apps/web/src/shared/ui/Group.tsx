import { type ReactNode, useId } from "react";

/**
 * A grouped inset list, as in System Settings: a small title above a
 * rounded box whose rows are divided by hairlines, and an optional note
 * below. Rows are its children, each a `GroupRow` or any block.
 */
export function Group({
  title,
  note,
  children,
}: {
  readonly title?: string;
  readonly note?: ReactNode;
  readonly children: ReactNode;
}) {
  const heading = useId();
  return (
    <section aria-labelledby={title === undefined ? undefined : heading}>
      {title !== undefined && (
        <h2 id={heading} className="px-3 pb-2 text-[13px] font-semibold text-ink-2">
          {title}
        </h2>
      )}
      <div className="divide-y divide-line overflow-hidden rounded-xl border border-line bg-surface">
        {children}
      </div>
      {note !== undefined && <div className="px-3 pt-2 text-[12px] text-ink-3">{note}</div>}
    </section>
  );
}

/** One row of a `Group`: an icon tile, a title and description, and a control on the right. */
export function GroupRow({
  icon,
  title,
  description,
  trailing,
  titleId,
  descriptionId,
}: {
  readonly icon?: ReactNode;
  readonly title: ReactNode;
  readonly description?: ReactNode;
  readonly trailing?: ReactNode;
  readonly titleId?: string;
  readonly descriptionId?: string;
}) {
  return (
    <div className="flex flex-wrap items-center gap-x-3 gap-y-2 px-4 py-3">
      {icon !== undefined && <div className="self-start">{icon}</div>}
      <div className="min-w-0 flex-1 basis-48">
        <p id={titleId} className="text-[14px] font-medium text-ink">
          {title}
        </p>
        {description !== undefined && (
          <div id={descriptionId} className="mt-0.5 text-[13px] leading-snug text-ink-2">
            {description}
          </div>
        )}
      </div>
      {trailing !== undefined && <div className="ml-auto shrink-0">{trailing}</div>}
    </div>
  );
}

/** A small coloured tile behind an icon, as System Settings shows beside each row. */
export function IconTile({
  children,
  tone = "accent",
}: {
  readonly children: ReactNode;
  readonly tone?: "accent" | "neutral" | "amber" | "green";
}) {
  const color = {
    accent: "bg-accent text-white",
    neutral: "bg-neutral-500 text-white dark:bg-neutral-600",
    amber: "bg-amber-500 text-white",
    green: "bg-emerald-500 text-white dark:bg-emerald-600",
  }[tone];
  return (
    <span
      className={`flex size-7 shrink-0 items-center justify-center rounded-[7px] ${color} [&>svg]:size-[18px]`}
    >
      {children}
    </span>
  );
}
