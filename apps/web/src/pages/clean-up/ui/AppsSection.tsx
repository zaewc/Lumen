import type { SoftwareItemView } from "@lumen/api-client";
import { useQuery } from "@tanstack/react-query";
import { Link } from "@tanstack/react-router";
import { useId, useState } from "react";
import { MoveFlow, MoveResult, softwareTitle } from "@/entities/app-move";
import { useTransport } from "@/shared/api";
import { useI18n } from "@/shared/i18n";
import { formatBytes, parentPath } from "@/shared/lib";
import { Bytes } from "@/shared/ui";
import { type AppRow, appsSection } from "../model/apps";

function Row({
  row,
  onOpen,
}: {
  readonly row: AppRow;
  readonly onOpen: (item: SoftwareItemView) => void;
}) {
  const { m } = useI18n();
  const folder = row.item.location === null ? "" : parentPath(row.item.location);
  return (
    <li>
      <button
        type="button"
        aria-haspopup="dialog"
        onClick={() => onOpen(row.item)}
        className="grid w-full grid-cols-[minmax(0,1fr)_auto] items-center gap-x-3 gap-y-1 rounded px-2 py-2.5 text-left transition-colors hover:bg-fill focus-visible:bg-accent-soft focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-accent active:bg-fill-strong motion-reduce:transition-none @xl:grid-cols-[minmax(0,2fr)_minmax(0,3fr)_8rem]"
      >
        <span className="truncate font-medium text-ink">{softwareTitle(row.item)}</span>
        <span className="col-start-2 row-start-1 justify-self-end text-right whitespace-nowrap tabular-nums text-ink @xl:col-start-3">
          {row.atLeast ? m.details.atLeast(formatBytes(row.bytes)) : <Bytes value={row.bytes} />}
        </span>
        <span
          title={folder}
          className="row-start-2 truncate text-[12px] text-ink-3 @xl:col-start-2 @xl:row-start-1 @xl:text-[13px]"
        >
          {folder}
        </span>
      </button>
    </li>
  );
}

/**
 * The installed apps the scan measured (roadmap 16.2, option A): after the
 * policy's sections, largest first, each opening the same move dialog as the
 * Apps page. Apps have no verdict, so they are outside the status filter, the
 * map and the "can clean up" total; the toolbar's "정리하기…" never moves them.
 * Left out where the host has no software list or the scan measured no app.
 */
export function AppsSection() {
  const { m } = useI18n();
  const t = m.results.apps;
  const heading = useId();
  const transport = useTransport();
  const ask = transport.software;
  const software = useQuery({
    queryKey: ["software"],
    queryFn: () => (ask === undefined ? Promise.reject(new Error("unsupported")) : ask()),
    enabled: ask !== undefined,
  });
  const [moving, setMoving] = useState<SoftwareItemView | null>(null);
  const items = software.data?.items ?? [];
  const section = appsSection(items);
  if (section === null) {
    return null;
  }
  const name = (key: string) => {
    const item = items.find((each) => each.key === key);
    return item === undefined ? null : softwareTitle(item);
  };
  return (
    <section aria-labelledby={heading} className="@container space-y-2 border-b border-line pb-3">
      <h3
        id={heading}
        className="flex items-baseline justify-between px-2 pt-6 text-section text-ink"
      >
        <span>
          {t.title}
          <span className="ml-2 font-normal tabular-nums text-ink-3">
            {m.common.items(section.rows.length)}
          </span>
        </span>
        <span className="font-normal tabular-nums text-ink-2">
          <Bytes value={section.bytes} />
        </span>
      </h3>
      <p className="px-2 text-[12px] leading-snug text-ink-3">{t.line}</p>
      <MoveResult name={name} />
      {section.rows.length > 0 && (
        <ul className="divide-y divide-line">
          {section.rows.map((row) => (
            <Row key={row.item.key} row={row} onOpen={setMoving} />
          ))}
        </ul>
      )}
      <div className="space-y-1 px-2 text-[13px]">
        {section.outside > 0 && <p className="text-ink-2">{t.outside(section.outside)}</p>}
        <Link to="/apps" className="text-accent-ink hover:underline">
          {t.seeAll}
        </Link>
      </div>
      {moving !== null && <MoveFlow item={moving} onClose={() => setMoving(null)} />}
    </section>
  );
}
