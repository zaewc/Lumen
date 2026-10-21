import { Link } from "@tanstack/react-router";
import { useState } from "react";
import { heldTotal, useHeld } from "@/entities/held";
import { useI18n } from "@/shared/i18n";
import { baseName, formatBytes } from "@/shared/lib";
import { ArchiveIcon, buttonClass, ErrorNotice, InfoTip, Notice, Page } from "@/shared/ui";
import { doneText, doneWell, type LastAction } from "../lib/outcome";
import { HeldRow } from "./HeldRow";

/** Like a trash can: what quarantine holds, to restore or remove for good. */
export function QuarantinePage() {
  const { m } = useI18n();
  const held = useHeld();
  // Shown above the list, so it stays after a restored item leaves the list.
  const [last, setLast] = useState<LastAction | null>(null);
  const items = held.data ?? [];
  const total = heldTotal(items);
  const size = formatBytes(total.bytes);
  return (
    <Page
      title={m.quarantine.title}
      {...(items.length > 0
        ? {
            subtitle: (
              <span className="flex flex-wrap items-center gap-x-1.5">
                <span className="tabular-nums">{m.quarantine.count(items.length)}</span>
                <span aria-hidden="true">·</span>
                <span className="tabular-nums">
                  {total.exact ? m.quarantine.total(size) : m.quarantine.totalAtLeast(size)}
                </span>
                <InfoTip term="quarantine" />
              </span>
            ),
          }
        : {})}
    >
      <div role="status" className="empty:hidden">
        {last && (
          <Notice tone={doneWell(last.done) ? "success" : "warning"}>
            <p>
              {last.original !== null && (
                <span className="font-semibold">{baseName(last.original)}: </span>
              )}
              {doneText(m, last.done)}
            </p>
          </Notice>
        )}
      </div>
      {held.isPending && <p className="px-1 text-[13px] text-ink-3">{m.common.loading}</p>}
      {held.isError && <ErrorNotice message={m.quarantine.loadFailed} error={held.error} />}
      {held.isSuccess && items.length === 0 && (
        <section className="flex flex-col items-center rounded-xl border border-line bg-surface px-6 py-12 text-center">
          <span className="flex size-14 items-center justify-center rounded-2xl bg-fill-strong text-ink-2">
            <ArchiveIcon className="size-7" />
          </span>
          <h2 className="mt-5 text-[17px] font-semibold text-ink">{m.quarantine.empty}</h2>
          <p className="mt-1.5 max-w-sm text-[13px] leading-relaxed text-ink-2">
            {m.quarantine.emptyHint}
          </p>
          <Link to="/" className={`${buttonClass("secondary")} mt-6`}>
            {m.quarantine.goCleanUp}
          </Link>
        </section>
      )}
      {items.length > 0 && (
        <section>
          <div className="flex items-start gap-2 px-1 pb-2.5 text-[13px] text-ink-2">
            <p className="flex-1">{m.quarantine.intro}</p>
            <InfoTip term="restore" />
          </div>
          <ul className="divide-y divide-line overflow-hidden rounded-xl border border-line bg-surface">
            {items.map((item) => (
              <HeldRow key={item.operation} item={item} onDone={setLast} />
            ))}
          </ul>
        </section>
      )}
    </Page>
  );
}
