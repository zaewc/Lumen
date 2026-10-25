import { Link } from "@tanstack/react-router";
import { useState } from "react";
import { heldTotal, useHeld } from "@/entities/held";
import { useI18n } from "@/shared/i18n";
import { baseName, formatBytes } from "@/shared/lib";
import { buttonClass, ErrorNotice, InfoTip, Notice, Page } from "@/shared/ui";
import { doneText, doneWell, type LastAction } from "../lib/outcome";
import { HELD_GRID, HeldRow } from "./HeldRow";

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
      accessory={
        <>
          <InfoTip term="quarantine" />
          {items.length > 0 && (
            <span className="tabular-nums">
              {m.common.items(items.length)} · {total.exact ? size : m.quarantine.atLeast(size)}
            </span>
          )}
        </>
      }
    >
      <div role="status" className="empty:hidden">
        {last && (
          <Notice tone={doneWell(last.done) ? "success" : "warning"}>
            <p>
              {last.original !== null && (
                <span className="font-medium">{baseName(last.original)}: </span>
              )}
              {doneText(m, last.done)}
            </p>
          </Notice>
        )}
      </div>
      {held.isPending && <p className="text-ink-3">{m.common.loading}</p>}
      {held.isError && (
        <ErrorNotice message={m.quarantine.loadFailed} error={held.error}>
          <button
            type="button"
            onClick={() => void held.refetch()}
            className={buttonClass("secondary", "sm")}
          >
            {m.common.tryAgain}
          </button>
        </ErrorNotice>
      )}
      {held.isSuccess && items.length === 0 && (
        <section className="space-y-1">
          <h2 className="font-semibold text-ink">{m.quarantine.empty}</h2>
          <p className="text-ink-2">{m.quarantine.emptyHint}</p>
          <Link to="/clean" className="inline-block pt-1 text-accent-ink hover:underline">
            {m.quarantine.goCleanUp}
          </Link>
        </section>
      )}
      {items.length > 0 && (
        <div className="@container">
          <div
            aria-hidden="true"
            className={`${HELD_GRID} hidden border-b border-line pb-1 text-[12px] font-medium text-ink-3 @2xl:grid`}
          >
            <span>{m.quarantine.columns.name}</span>
            <span>{m.quarantine.columns.location}</span>
            <span className="text-right">{m.quarantine.columns.size}</span>
            <span>{m.quarantine.columns.held}</span>
            <span />
          </div>
          <ul className="divide-y divide-line border-b border-line">
            {items.map((item) => (
              <HeldRow key={item.operation} item={item} onDone={setLast} />
            ))}
          </ul>
        </div>
      )}
    </Page>
  );
}
