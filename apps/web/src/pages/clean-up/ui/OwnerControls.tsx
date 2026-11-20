import { useId, useRef } from "react";
import { useI18n } from "@/shared/i18n";
import { useShortcut } from "@/shared/lib";
import { buttonClass, Notice } from "@/shared/ui";
import { partial } from "../model/apps-list";
import type { OwnerOrder } from "../model/owners";
import type { Software } from "./useSoftware";

/**
 * Above the Storage list (ADR-0069): a search across owners, apps, packages
 * and items (⌘F or Ctrl+F), the order, and reading the installed software
 * again.
 */
export function OwnerControls({
  query,
  onQuery,
  order,
  onOrder,
  software,
}: {
  readonly query: string;
  readonly onQuery: (query: string) => void;
  readonly order: OwnerOrder;
  readonly onOrder: (order: OwnerOrder) => void;
  readonly software: Software;
}) {
  const { m } = useI18n();
  const a = m.apps;
  const searchId = useId();
  const search = useRef<HTMLInputElement>(null);
  useShortcut("f", () => search.current?.focus());
  return (
    <div className="flex flex-wrap items-center gap-x-6 gap-y-2">
      <label htmlFor={searchId} className="sr-only">
        {a.search}
      </label>
      <input
        ref={search}
        id={searchId}
        type="search"
        value={query}
        onChange={(event) => onQuery(event.target.value)}
        placeholder={m.results.search}
        className="h-9 w-full max-w-xs rounded-lg border border-line-strong bg-canvas px-3 text-[13px] text-ink focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-accent"
      />
      <fieldset className="flex items-center gap-3 text-[13px] text-ink-2">
        <legend className="sr-only">{a.order.label}</legend>
        {(["size", "name"] as const).map((each) => (
          <label key={each} className="flex items-center gap-1.5">
            <input
              type="radio"
              name="owners-order"
              checked={order === each}
              onChange={() => onOrder(each)}
            />
            {a.order[each]}
          </label>
        ))}
      </fieldset>
      {software.available && (
        <button
          type="button"
          disabled={software.reading}
          onClick={software.readAgain}
          className={`${buttonClass("secondary", "sm")} ml-auto`}
        >
          {software.reading ? a.reading : a.readAgain}
        </button>
      )}
      {software.list !== undefined && partial(software.list.sources).length > 0 && (
        <div className="w-full">
          <Notice tone="warning">
            <p>{a.partial(partial(software.list.sources).join(", "))}</p>
          </Notice>
        </div>
      )}
      {software.explaining && (
        <p role="status" className="w-full text-[12px] text-ink-2">
          {a.ai.explaining}
        </p>
      )}
    </div>
  );
}
