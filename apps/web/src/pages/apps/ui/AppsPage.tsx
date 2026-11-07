import type { SoftwareItemView } from "@lumen/api-client";
import { useQuery } from "@tanstack/react-query";
import { useId, useState } from "react";
import { useAiSettings } from "@/entities/ai-review";
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
        {item.ai_note !== null && (
          <p className="text-[12px] text-ink-2">
            {a.ai.note(
              a.ai.purposes[item.ai_note.purpose],
              m.aiNote.confidence[item.ai_note.confidence],
              a.ai.impacts[item.ai_note.removal_impact],
            )}
          </p>
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
  const transport = useTransport();
  const ask = transport.software;
  const explain = transport.explainSoftware;
  const aiOn = useAiSettings().data?.enabled === true;
  const searchId = useId();
  const [query, setQuery] = useState("");
  const [all, setAll] = useState(false);
  const software = useQuery({
    queryKey: ["software"],
    queryFn: () => (ask === undefined ? Promise.reject(new Error("unsupported")) : ask()),
    enabled: ask !== undefined,
    retry: false,
  });
  // While AI review is on, explain what has no note yet once the list is in
  // (ADR-0061); the list shows first and the notes fill in when they come.
  const explained = useQuery({
    queryKey: ["software", "explained"],
    queryFn: () => (explain === undefined ? Promise.reject(new Error("unsupported")) : explain()),
    enabled: explain !== undefined && aiOn && software.isSuccess,
    retry: false,
    refetchOnWindowFocus: false,
  });
  const list = explained.data ?? software.data;
  const items = list?.items ?? [];
  const shown = visible(items, { query, all });
  const missing = partial(list?.sources ?? []);
  const hiddenCount = tucked(items);

  return (
    <Page
      title={a.title}
      accessory={
        list !== undefined && <span className="tabular-nums">{m.common.items(items.length)}</span>
      }
      actions={
        ask !== undefined && (
          <button
            type="button"
            disabled={software.isFetching}
            onClick={() =>
              void software.refetch().then(() => {
                if (explain !== undefined && aiOn) void explained.refetch();
              })
            }
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
      {list !== undefined && (
        <div className="space-y-4">
          {explained.isFetching && (
            <p role="status" className="text-[12px] text-ink-2">
              {a.ai.explaining}
            </p>
          )}
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
          <p className="text-[12px] text-ink-3">{aiOn ? a.readOnlyAi : a.readOnly}</p>
        </div>
      )}
    </Page>
  );
}
