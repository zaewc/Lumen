import { type ReactNode, useId } from "react";

/**
 * A group of settings or figures: a small title, then its rows divided by
 * hairlines (no box around them), and an optional note below. Without a
 * title, the group starts with its first row. Rows are its
 * children, each a `GroupRow` or any block.
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
        <h2 id={heading} className="pb-1 text-[13px] font-semibold text-ink">
          {title}
        </h2>
      )}
      <div
        className={`divide-y divide-line border-b border-line ${title === undefined ? "" : "border-t"}`}
      >
        {children}
      </div>
      {note !== undefined && <div className="pt-2 text-[12px] text-ink-3">{note}</div>}
    </section>
  );
}

/** One row of a `Group`: a title and description, and a control on the right. */
export function GroupRow({
  title,
  description,
  trailing,
  titleId,
  descriptionId,
}: {
  readonly title: ReactNode;
  readonly description?: ReactNode;
  readonly trailing?: ReactNode;
  readonly titleId?: string;
  readonly descriptionId?: string;
}) {
  return (
    <div className="flex flex-wrap items-center gap-x-4 gap-y-2 py-2.5">
      <div className="min-w-0 flex-1 basis-56">
        <p id={titleId} className="text-[13px] font-medium text-ink">
          {title}
        </p>
        {description !== undefined && (
          <div id={descriptionId} className="mt-0.5 text-[12px] leading-snug text-ink-2">
            {description}
          </div>
        )}
      </div>
      {trailing !== undefined && <div className="ml-auto shrink-0">{trailing}</div>}
    </div>
  );
}
