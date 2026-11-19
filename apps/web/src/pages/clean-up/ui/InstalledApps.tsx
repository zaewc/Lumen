import type { SoftwareItemView } from "@lumen/api-client";
import { useQuery } from "@tanstack/react-query";
import { useId, useState } from "react";
import { useAiSettings } from "@/entities/ai-review";
import { useTransport } from "@/shared/api";
import { softwareNoteText, useI18n } from "@/shared/i18n";
import { formatBytes } from "@/shared/lib";
import { Bytes, buttonClass, ErrorNotice, Notice } from "@/shared/ui";
import {
  iconsShown,
  type Order,
  ordered,
  partial,
  title,
  tucked,
  visible,
} from "../model/apps-list";
import { removalFacts } from "../model/facts";
import { AppIcon } from "./AppIcon";
import { MoveFlow, MoveResult } from "./MoveFlow";

function AppRow({
  item,
  icon,
  onMove,
}: {
  readonly item: SoftwareItemView;
  readonly icon: boolean;
  readonly onMove: ((item: SoftwareItemView) => void) | null;
}) {
  const { m } = useI18n();
  const a = m.apps;
  const details = [
    a.origins[item.origin],
    item.name !== null ? item.id : null,
    item.version,
    item.vendor,
  ].filter((part): part is string => part !== null);
  const facts = removalFacts(m, item);
  return (
    <li className="flex items-start justify-between gap-4 py-3">
      <div className="flex min-w-0 items-start gap-3">
        {icon && <AppIcon item={item} />}
        <div className="min-w-0 space-y-0.5">
          <p className="truncate text-[14px] font-medium text-ink">{title(item)}</p>
          <p className="truncate text-[12px] text-ink-2">{details.join(" · ")}</p>
          {item.location !== null && (
            <p className="truncate font-mono text-[11px] text-ink-3">{item.location}</p>
          )}
          {item.size !== null && item.size.kind !== "not_scanned" && (
            <p className="text-[12px] tabular-nums text-ink-2">
              {item.size.kind === "at_least" ? (
                m.details.atLeast(formatBytes(item.size.bytes))
              ) : (
                <Bytes value={item.size.bytes} />
              )}
            </p>
          )}
          {facts.length > 0 && (
            <div className="pt-1">
              <p className="text-[11px] font-medium text-ink-3">{a.facts.title}</p>
              <ul className="text-[12px] text-ink-2">
                {facts.map((fact) => (
                  <li key={fact.text}>
                    {fact.text} <span className="text-ink-3">· {fact.source}</span>
                  </li>
                ))}
              </ul>
            </div>
          )}
          {item.ai_note !== null && (
            <div className="pt-1">
              <p className="text-[12px] text-ink-2">{softwareNoteText(m, item.ai_note)}</p>
              <p className="text-[11px] text-ink-3">{a.ai.source}</p>
            </div>
          )}
        </div>
      </div>
      {item.protected ? (
        <span title={a.protectedTip} className="shrink-0 text-[12px] text-ink-2">
          {a.protected}
        </span>
      ) : (
        onMove !== null &&
        item.origin === "mac_app" && (
          <button
            type="button"
            onClick={() => onMove(item)}
            className={`${buttonClass("secondary", "sm")} shrink-0`}
          >
            {a.move.action}
          </button>
        )
      )}
    </li>
  );
}

/**
 * Everything the platform says is installed (ADR-0060), as a section of
 * 저장 공간 (ADR-0069): applications and packages from each inventory, read
 * when it opens. Nothing moves
 * without a confirmed preview: a user-owned macOS app can be moved to
 * Quarantine (ADR-0062), and the host decides whether it may. Absent where
 * the host cannot read inventories (a browser).
 */
export function InstalledApps() {
  const { m } = useI18n();
  const a = m.apps;
  const transport = useTransport();
  const ask = transport.software;
  const explain = transport.explainSoftware;
  const aiOn = useAiSettings().data?.enabled === true;
  const searchId = useId();
  const [query, setQuery] = useState("");
  const [all, setAll] = useState(false);
  const [order, setOrder] = useState<Order>("name");
  const [moving, setMoving] = useState<SoftwareItemView | null>(null);
  const canMove = transport.uninstallPreview !== undefined && transport.uninstall !== undefined;
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
  const shown = ordered(visible(items, { query, all }), order);
  const anySized = items.some((item) => item.size !== null && item.size.kind !== "not_scanned");
  const missing = partial(list?.sources ?? []);
  const hiddenCount = tucked(items);
  const icons = transport.appIcon !== undefined && iconsShown(items);

  const heading = useId();
  return (
    <section aria-labelledby={heading} className="space-y-3">
      <div className="flex flex-wrap items-baseline justify-between gap-3">
        <h2 id={heading} className="text-section text-ink">
          {a.title}
          {list !== undefined && (
            <span className="ml-2 font-normal tabular-nums text-ink-3">
              {m.common.items(items.length)}
            </span>
          )}
        </h2>
        {ask !== undefined && (
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
        )}
      </div>
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
          <MoveResult
            name={(key) => {
              const found = items.find((item) => item.key === key);
              return found === undefined ? null : title(found);
            }}
          />
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
            {anySized && (
              <fieldset className="flex items-center gap-3 text-[13px] text-ink-2">
                <legend className="sr-only">{a.order.label}</legend>
                {(["name", "size"] as const).map((each) => (
                  <label key={each} className="flex items-center gap-1.5">
                    <input
                      type="radio"
                      name="apps-order"
                      checked={order === each}
                      onChange={() => setOrder(each)}
                    />
                    {a.order[each]}
                  </label>
                ))}
              </fieldset>
            )}
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
                <AppRow
                  key={item.key}
                  item={item}
                  icon={icons}
                  onMove={canMove ? setMoving : null}
                />
              ))}
            </ul>
          )}
          <p className="text-[12px] text-ink-3">{aiOn ? a.readOnlyAi : a.readOnly}</p>
        </div>
      )}
      {moving !== null && <MoveFlow item={moving} onClose={() => setMoving(null)} />}
    </section>
  );
}
