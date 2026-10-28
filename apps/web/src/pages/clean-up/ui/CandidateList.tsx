import type { CandidateView } from "@lumen/api-client";
import { type KeyboardEvent, type ReactNode, useId, useRef, useState } from "react";
import { statusOf } from "@/entities/scan-session";
import { aiNoteText, useI18n } from "@/shared/i18n";
import { baseName, formatBytes, parentPath } from "@/shared/lib";
import { commandLine } from "../lib/command";
import { moveIndex } from "../lib/keys";
import type { KindSection, SourceGroup } from "../model/sections";
import { StatusText } from "./Status";

/**
 * The list's columns. Wide: name, location, status, size on one line.
 * Narrow (below 36rem): name and size, then location and status under them.
 */
const GRID =
  "grid grid-cols-[minmax(0,1fr)_auto] gap-x-3 @xl:grid-cols-[minmax(0,2fr)_minmax(0,3fr)_6rem_5.5rem]";

/**
 * One candidate; Enter, Space or a click opens its details. An item with its
 * own command shows the command under it, to read or select; copying it is
 * in the details. An item to review that AI review described shows the note
 * under it, muted.
 */
function Row({
  candidate,
  index,
  active,
  inspected,
  largest,
  hint,
  onOpen,
  onMove,
}: {
  readonly candidate: CandidateView;
  readonly index: number;
  /** The row the keyboard is on: the list's one tab stop. */
  readonly active: boolean;
  readonly inspected: boolean;
  readonly largest: bigint;
  readonly hint: string;
  readonly onOpen: (candidate: CandidateView) => void;
  readonly onMove: (event: KeyboardEvent, from: number) => void;
}) {
  const status = statusOf(candidate);
  const keep = status === "keep";
  const location = parentPath(candidate.path);
  const command = status === "command" ? candidate.tool_command : null;
  const note = status === "review" ? candidate.ai_note : null;
  const { m } = useI18n();
  const footprint =
    largest === 0n ? 0 : Number((BigInt(candidate.reclaimable_bytes) * 1000n) / largest) / 10;
  return (
    <li>
      <button
        type="button"
        data-row={index}
        data-inspected={inspected}
        aria-haspopup="dialog"
        tabIndex={active ? 0 : -1}
        aria-describedby={active ? hint : undefined}
        onClick={() => onOpen(candidate)}
        onKeyDown={(event) => onMove(event, index)}
        className={`${GRID} w-full items-center gap-y-1 rounded px-2 py-2.5 text-left transition-colors hover:bg-fill data-[inspected=true]:bg-accent-soft focus-visible:bg-accent-soft focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-accent active:bg-fill-strong motion-reduce:transition-none`}
      >
        <span
          title={candidate.path}
          className={`truncate font-medium ${candidate.group.kind === "developer" ? "border-l border-line-strong pl-3" : ""} ${keep ? "text-ink-2" : "text-ink"}`}
        >
          {baseName(candidate.path)}
        </span>
        <span className="col-start-2 row-start-1 justify-self-end text-right tabular-nums text-ink @xl:col-start-4">
          {formatBytes(candidate.reclaimable_bytes)}
          <span aria-hidden="true" className="mt-1 hidden h-0.5 w-14 bg-line @xl:block">
            <span className="block h-full bg-ink-3" style={{ width: `${footprint}%` }} />
          </span>
        </span>
        <span
          title={location}
          className="row-start-2 truncate text-[12px] text-ink-3 @xl:col-start-2 @xl:row-start-1 @xl:text-[13px]"
        >
          {location}
        </span>
        <span className="col-start-2 row-start-2 text-right text-[12px] @xl:col-start-3 @xl:row-start-1 @xl:text-left @xl:text-[13px]">
          <StatusText status={status} />
        </span>
      </button>
      {command !== null && (
        <code className="mx-2 mb-2 block font-mono text-[12px] leading-relaxed break-all text-ink-2 select-all">
          {commandLine(command)}
        </code>
      )}
      {note !== null && (
        <p className="mx-2 mb-1.5 truncate text-[12px] text-ink-3">{aiNoteText(m, note)}</p>
      )}
    </li>
  );
}

/** A header inside the list: a section's or a tool's name, count and size. */
function ListHeading({
  level,
  id,
  name,
  count,
  bytes,
}: {
  readonly level: 3 | 4;
  readonly id?: string;
  readonly name: string;
  readonly count: number;
  readonly bytes: string;
}) {
  const { m } = useI18n();
  const Tag = level === 3 ? "h3" : "h4";
  return (
    <Tag
      id={id}
      className={`${GRID} items-baseline px-2 ${level === 3 ? "pt-6 pb-2 text-[15px] font-semibold text-ink" : "pt-3 pb-1 text-[12px] font-medium text-ink-2"}`}
    >
      <span className="truncate @xl:col-span-3">
        {name}
        <span className="ml-2 font-normal tabular-nums text-ink-3">{m.common.items(count)}</span>
      </span>
      <span className="text-right font-normal tabular-nums text-ink-2">{formatBytes(bytes)}</span>
    </Tag>
  );
}

/**
 * The candidates in sections by kind (developer tools by tool), in aligned
 * columns, or a note when the filter leaves none. It is one stop for Tab:
 * ↑ and ↓ (and Page Up, Page Down, Home, End) move between rows.
 */
export function CandidateList({
  sections,
  onOpen,
  selectedPath,
}: {
  readonly sections: readonly KindSection[];
  readonly onOpen: (candidate: CandidateView) => void;
  readonly selectedPath: string | null;
}) {
  const { m } = useI18n();
  const hint = useId();
  const list = useRef<HTMLDivElement>(null);
  const [selected, setSelected] = useState(0);
  const count = sections.reduce((total, section) => total + section.count, 0);
  // A filter may have shortened the list since.
  const active = Math.min(selected, count - 1);

  if (sections.length === 0) {
    return <p className="py-6 text-center text-ink-3">{m.results.emptyFilter}</p>;
  }

  const onMove = (event: KeyboardEvent, from: number) => {
    const next = moveIndex(from, count, event.key);
    if (next === null || event.altKey || event.metaKey || event.ctrlKey) {
      return;
    }
    event.preventDefault();
    setSelected(next);
    list.current?.querySelector<HTMLElement>(`[data-row="${next}"]`)?.focus();
  };

  // Each row's place in reading order, across sections and tools.
  const ordered = sections.flatMap((section) => section.sources.flatMap((group) => group.items));
  const order = new Map(ordered.map((candidate, at) => [candidate.path, at]));
  const largest = ordered.reduce(
    (size, item) => (BigInt(item.reclaimable_bytes) > size ? BigInt(item.reclaimable_bytes) : size),
    0n,
  );
  const rows = (group: SourceGroup) => (
    <ul className="divide-y divide-line">
      {group.items.map((candidate) => {
        const at = order.get(candidate.path) ?? 0;
        return (
          <Row
            key={candidate.path}
            candidate={candidate}
            index={at}
            active={at === active}
            inspected={candidate.path === selectedPath}
            largest={largest}
            hint={hint}
            onOpen={(opened) => {
              setSelected(at);
              onOpen(opened);
            }}
            onMove={onMove}
          />
        );
      })}
    </ul>
  );

  return (
    <div ref={list} className="@container">
      <p id={hint} className="sr-only">
        {m.results.listHint}
      </p>
      <div
        aria-hidden="true"
        className={`${GRID} hidden border-b border-line px-2 pb-1 text-[12px] font-medium text-ink-3 @xl:grid`}
      >
        <span>{m.results.columns.name}</span>
        <span>{m.results.columns.location}</span>
        <span>{m.results.columns.status}</span>
        <span className="text-right">{m.results.columns.size}</span>
      </div>
      {sections.map((section) => (
        <Section key={section.kind} section={section} rows={rows} />
      ))}
    </div>
  );
}

/** One kind of group: its heading with totals, then its rows (by tool, for developer tools). */
function Section({
  section,
  rows,
}: {
  readonly section: KindSection;
  readonly rows: (group: SourceGroup) => ReactNode;
}) {
  const { m } = useI18n();
  const heading = useId();
  return (
    <section aria-labelledby={heading} className="border-b border-line">
      <ListHeading
        level={3}
        id={heading}
        name={m.results.groups[section.kind]}
        count={section.count}
        bytes={section.bytes}
      />
      {section.kind === "command" && (
        <p className="px-2 pb-1.5 text-[12px] leading-snug text-ink-3">{m.results.commandNote}</p>
      )}
      {section.kind === "developer"
        ? section.sources.map((group) => (
            <div key={group.source ?? ""}>
              <ListHeading
                level={4}
                name={group.source ?? m.results.otherTools}
                count={group.items.length}
                bytes={group.bytes}
              />
              {rows(group)}
            </div>
          ))
        : section.sources.map((group) => <div key={group.source ?? ""}>{rows(group)}</div>)}
    </section>
  );
}
