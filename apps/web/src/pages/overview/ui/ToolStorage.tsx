import type { ToolStorageView } from "@lumen/api-client";
import { useQuery } from "@tanstack/react-query";
import { useId } from "react";
import { useTransport } from "@/shared/api";
import { useI18n } from "@/shared/i18n";
import { Bytes, buttonClass, ErrorNotice } from "@/shared/ui";
import { itemSize, TOOL_NAMES, versionText } from "../model/tools";

/** What the tool said, or why it said nothing; unknown is never shown as empty. */
function ToolRow({ view }: { readonly view: ToolStorageView }) {
  const { m } = useI18n();
  const t = m.overview.tools;
  const version = versionText(view);
  const state = view.state;
  const status = (() => {
    switch (state.kind) {
      case "found":
        return view.items.length === 0 ? t.none : null;
      case "not_installed":
        return t.notInstalled;
      case "unsupported":
        return t.unsupported;
      case "unavailable":
        return t.unavailable;
      default: {
        const unknown: never = state;
        return unknown;
      }
    }
  })();
  const note = (() => {
    if (state.kind !== "found") return null;
    switch (view.tool) {
      case "xcode":
        return t.xcodeManage;
      case "docker":
        return t.dockerManage;
      case "homebrew":
      case "npm":
      case "pnpm":
        return view.shared_store ? t.sharedStoreNote : t.cacheNote;
      default: {
        const unknown: never = view.tool;
        return unknown;
      }
    }
  })();
  const packageManager = view.tool !== "xcode" && view.tool !== "docker";
  return (
    <li className="space-y-2 py-4">
      <div className="flex items-baseline justify-between gap-3">
        <h3 className="text-[14px] font-semibold text-ink">
          {TOOL_NAMES[view.tool]}
          {version !== null && (
            <span className="ml-2 text-[12px] font-normal text-ink-2 tabular-nums">{version}</span>
          )}
        </h3>
        {status !== null && <span className="text-[13px] text-ink-2">{status}</span>}
        {status === null && packageManager && (
          <span className="text-[12px] text-ink-2">
            {view.shared_store ? t.sharedStore : t.cache}
          </span>
        )}
      </div>
      {state.kind === "unavailable" && (
        <p className="font-mono text-[12px] break-all text-ink-3 select-text">{state.reason}</p>
      )}
      {view.scope !== null && (
        <p className="font-mono text-[12px] break-all text-ink-3">{view.scope}</p>
      )}
      {view.items.length > 0 && (
        <ul className="space-y-1 text-[13px] text-ink">
          {view.items.map((item) => {
            const size = itemSize(item);
            return (
              <li key={item.name} className="flex flex-wrap items-baseline justify-between gap-x-4">
                <span className={packageManager ? "font-mono text-[12px] break-all" : ""}>
                  {item.name}
                  {item.detail !== null && (
                    <span className="ml-2 text-ink-2">
                      {view.tool === "docker" ? t.inUse(item.detail) : item.detail}
                    </span>
                  )}
                </span>
                <span className="text-ink-2 tabular-nums">
                  {size.kind === "bytes" && <Bytes value={size.value} />}
                  {size.kind === "reported" && size.text}
                  {item.reported_reclaimable !== null && (
                    <span className="ml-3">{t.reclaimable(item.reported_reclaimable)}</span>
                  )}
                </span>
              </li>
            );
          })}
        </ul>
      )}
      {note !== null && <p className="text-[12px] text-ink-2">{note}</p>}
      {state.kind === "found" && view.command !== null && (
        <p className="text-[12px] text-ink-2">
          {t.ownCommand}:{" "}
          <code className="font-mono break-all text-ink select-all">{view.command}</code>
        </p>
      )}
    </li>
  );
}

/**
 * Developer tools asked about their own storage, only on request: the host runs
 * fixed read-only queries, and nothing here removes anything. Absent where the
 * host cannot run them (a browser).
 */
export function ToolStorage() {
  const { m } = useI18n();
  const ask = useTransport().toolStorage;
  const title = useId();
  const tools = useQuery({
    queryKey: ["tool-storage"],
    queryFn: () => (ask === undefined ? Promise.resolve([]) : ask()),
    enabled: false,
    retry: false,
  });
  if (ask === undefined) return null;
  const t = m.overview.tools;
  const checked = tools.data !== undefined;
  return (
    <section aria-labelledby={title} aria-busy={tools.isFetching} className="space-y-3 pt-8">
      <div className="flex items-baseline justify-between gap-3">
        <h2 id={title} className="text-section text-ink">
          {t.title}
        </h2>
        <button
          type="button"
          disabled={tools.isFetching}
          onClick={() => void tools.refetch()}
          className={buttonClass("secondary", "sm")}
        >
          {tools.isFetching ? t.checking : checked ? t.checkAgain : t.check}
        </button>
      </div>
      {!checked && !tools.isError && <p className="text-[13px] text-ink-2">{t.intro}</p>}
      {tools.isError && <ErrorNotice message={t.loadFailed} error={tools.error} />}
      {checked && (
        <ul className="divide-y divide-line">
          {tools.data.map((view) => (
            <ToolRow key={view.tool} view={view} />
          ))}
        </ul>
      )}
    </section>
  );
}
