import type { SoftwareItemView } from "@lumen/api-client";
import { useQuery } from "@tanstack/react-query";
import { useId, useState } from "react";
import { useTransport } from "@/shared/api";
import { useI18n } from "@/shared/i18n";
import { buttonClass, ErrorNotice, Notice, Page } from "@/shared/ui";
import { partial, title, tucked, visible } from "../model/list";

function AppRow({ item }: { readonly item: SoftwareItemView }) {
  const { m } = useI18n();
  const a = m.apps;
  const details = [
    a.origins[item.origin],
    item.name !== null ? item.id : null,
    item.version,
    item.vendor,
  ].filter((part): part is string => part !== null);
  return (
    <li className="flex items-start justify-between gap-4 py-3">
      <div className="min-w-0 space-y-0.5">
        <p className="truncate text-[14px] font-medium text-ink">{title(item)}</p>
        <p className="truncate text-[12px] text-ink-2">{details.join(" · ")}</p>
        {item.location !== null && (
          <p className="truncate font-mono text-[11px] text-ink-3">{item.location}</p>
        )}
      </div>
      {item.protected && (
        <span title={a.protectedTip} className="shrink-0 text-[12px] text-ink-2">
          {a.protected}
        </span>
      )}
    </li>
  );
}

/**
 * Everything the platform says is installed (ADR-0060): applications and
 * packages from each inventory, read when the page opens. Read only; this
 * page removes nothing. Absent where the host cannot read inventories (a
 * browser).
 */
export function AppsPage() {
  const { m } = useI18n();
  const a = m.apps;
  const ask = useTransport().software;
  const searchId = useId();
  const [query, setQuery] = useState("");
  const [all, setAll] = useState(false);
  const software = useQuery({
    queryKey: ["software"],
    queryFn: () => (ask === undefined ? Promise.reject(new Error("unsupported")) : ask()),
    enabled: ask !== undefined,
    retry: false,
  });
  const items = software.data?.items ?? [];
  const shown = visible(items, { query, all });
  const missing = partial(software.data?.sources ?? []);
  const hiddenCount = tucked(items);

  return (
    <Page
      title={a.title}
      accessory={
        software.data !== undefined && (
          <span className="tabular-nums">{m.common.items(items.length)}</span>
        )
      }
      actions={
        ask !== undefined && (
          <button
            type="button"
            disabled={software.isFetching}
            onClick={() => void software.refetch()}
            className={buttonClass("secondary", "sm")}
          >
            {software.isFetching ? a.reading : a.readAgain}
          </button>
        )
      }
    >
      {ask === undefined && <p className="text-[13px] text-ink-2">{a.desktopOnly}</p>}
      {software.isError && ask !== undefined && (
        <ErrorNotice message={a.loadFailed} error={software.error} />
      )}
      {software.isPending && ask !== undefined && (
        <p role="status" className="text-[13px] text-ink-2">
          {a.reading}
        </p>
      )}
      {software.data !== undefined && (
        <div className="space-y-4">
          {missing.length > 0 && (
            <Notice tone="warning">
              <p>{a.partial(missing.join(", "))}</p>
            </Notice>
          )}
          <div className="flex flex-wrap items-center gap-x-6 gap-y-2">
            <label htmlFor={searchId} className="sr-only">
              {a.search}
            </label>
            <input
              id={searchId}
              type="search"
              value={query}
              onChange={(event) => setQuery(event.target.value)}
              placeholder={a.search}
              className="h-9 w-full max-w-xs rounded-lg border border-line-strong bg-canvas px-3 text-[13px] text-ink focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-accent"
            />
            {hiddenCount > 0 && (
              <label className="flex items-center gap-2 text-[13px] text-ink-2">
                <input type="checkbox" checked={all} onChange={() => setAll(!all)} />
                {a.showAll(hiddenCount)}
              </label>
            )}
          </div>
          {shown.length === 0 ? (
            <p className="text-[13px] text-ink-2">{a.none}</p>
          ) : (
            <ul className="divide-y divide-line">
              {shown.map((item) => (
                <AppRow key={item.key} item={item} />
              ))}
            </ul>
          )}
          <p className="text-[12px] text-ink-3">{a.readOnly}</p>
        </div>
      )}
    </Page>
  );
}
