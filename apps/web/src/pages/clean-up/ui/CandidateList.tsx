import type { CandidateView, SoftwareItemView } from "@lumen/api-client";
import { type KeyboardEvent, type ReactNode, useId, useRef, useState } from "react";
import { statusOf } from "@/entities/scan-session";
import { aiNoteText, useI18n } from "@/shared/i18n";
import { baseName, formatBytes, parentPath } from "@/shared/lib";
import { Bytes } from "@/shared/ui";
import { commandLine } from "../lib/command";
import { moveIndex } from "../lib/keys";
import { softwareTitle } from "../model/app-title";
import { type Lens, type LensMark, lensMark } from "../model/lens";
import type { Owner, OwnerPart } from "../model/owners";
import { type AppCheck, checkable } from "../model/selection";
import { CandidateSize } from "./CandidateSize";
import { StatusText } from "./Status";

/** The checkboxes of the list (ADR-0069). */
export interface Check {
  readonly isChecked: (candidate: CandidateView) => boolean;
  readonly toggle: (candidate: CandidateView) => void;
  /** A checked app's preview state, by software-list key. */
  readonly appState: (key: string) => AppCheck | undefined;
  readonly toggleApp: (key: string) => void;
}

/** The column every row keeps for a checkbox, so names line up. */
function CheckCell({
  candidate,
  check,
}: {
  readonly candidate: CandidateView;
  readonly check?: Check;
}) {
  const { m } = useI18n();
  return (
    <span className="flex w-7 shrink-0 justify-center pt-3">
      {check !== undefined && checkable(candidate) && (
        <input
          type="checkbox"
          checked={check.isChecked(candidate)}
          onChange={() => check.toggle(candidate)}
          aria-label={m.action.check(baseName(candidate.path))}
          className="size-4 cursor-pointer accent-accent"
        />
      )}
    </span>
  );
}

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
  mark,
  hint,
  moving,
  onOpen,
  onMove,
  check,
}: {
  readonly candidate: CandidateView;
  readonly check?: Check;
  /** It is being moved to Quarantine now. */
  readonly moving: boolean;
  readonly index: number;
  /** The row the keyboard is on: the list's one tab stop. */
  readonly active: boolean;
  /** How the open item's lens draws it, if an item is open. */
  readonly mark: LensMark | null;
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
  return (
    <li
      data-lens={mark ?? undefined}
      className="transition-opacity data-[lens=lowered]:opacity-40 motion-reduce:transition-none"
    >
      <div className="flex">
        <CheckCell candidate={candidate} {...(check === undefined ? {} : { check })} />
        <button
          type="button"
          data-row={index}
          data-inspected={mark === "inspected"}
          aria-haspopup="dialog"
          tabIndex={active ? 0 : -1}
          aria-describedby={active ? hint : undefined}
          onClick={() => onOpen(candidate)}
          onKeyDown={(event) => onMove(event, index)}
          className={`${GRID} min-w-0 flex-1 items-center gap-y-1 rounded px-2 py-2.5 text-left transition-colors hover:bg-fill data-[inspected=true]:bg-accent-soft data-[inspected=true]:outline-2 data-[inspected=true]:-outline-offset-2 data-[inspected=true]:outline-accent focus-visible:bg-accent-soft focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-accent active:bg-fill-strong motion-reduce:transition-none`}
        >
          <span
            title={candidate.path}
            className={`truncate border-l border-line-strong pl-3 font-medium ${keep ? "text-ink-2" : "text-ink"}`}
          >
            {baseName(candidate.path)}
          </span>
          <span className="col-start-2 row-start-1 justify-self-end text-right tabular-nums text-ink @xl:col-start-4">
            <CandidateSize candidate={candidate} />
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
      </div>
      {command !== null && (
        <code className="mr-2 mb-2 ml-9 block font-mono text-[12px] leading-relaxed break-all text-ink-2 select-all">
          {commandLine(command)}
        </code>
      )}
      {note !== null && (
        <p className="mr-2 mb-1.5 ml-9 truncate text-[12px] text-ink-3">{aiNoteText(m, note)}</p>
      )}
      {moving && (
        <div
          role="progressbar"
          aria-label={m.confirm.moving}
          className="mr-2 mb-2 ml-9 h-1 overflow-hidden rounded-full bg-accent-soft"
        >
          <div className="h-full w-2/5 animate-[indeterminate_1.4s_ease-in-out_infinite] rounded-full bg-accent motion-reduce:w-full motion-reduce:animate-none motion-reduce:opacity-50" />
        </div>
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
  exact,
  detail,
}: {
  readonly level: 3 | 4;
  readonly id?: string;
  readonly name: string;
  readonly count: number;
  /** Decimal bytes; `null` when no part's size is known. */
  readonly bytes: string | null;
  /** Whether `bytes` is exact; otherwise it shows as "at least". */
  readonly exact: boolean;
  /** A second line under the size, such as what is proposed. */
  readonly detail?: string;
}) {
  const { m } = useI18n();
  const Tag = level === 3 ? "h3" : "h4";
  return (
    <Tag
      id={id}
      className={`${GRID} items-baseline pr-2 pl-9 ${level === 3 ? "pt-6 pb-2 text-section text-ink" : "pt-3 pb-1 text-[12px] font-medium text-ink-2"}`}
    >
      <span className="truncate @xl:col-span-3">
        {name}
        <span className="ml-2 font-normal tabular-nums text-ink-3">{m.common.items(count)}</span>
        {detail !== undefined && (
          <span className="ml-2 text-[12px] font-normal tabular-nums text-accent-ink">
            · {detail}
          </span>
        )}
      </span>
      <span className="text-right font-normal tabular-nums text-ink-2">
        {bytes === null ? (
          m.results.apps.notMeasured
        ) : exact ? (
          <Bytes value={bytes} />
        ) : (
          m.details.atLeast(formatBytes(bytes))
        )}
      </span>
    </Tag>
  );
}

/**
 * An installed app itself (ADR-0069), under its owner: opens the app's own
 * move dialog, which checks it afresh (ADR-0062). It has no verdict.
 */
function AppRow({
  part,
  index,
  active,
  hint,
  onOpen,
  onDetails,
  onMove,
  check,
}: {
  readonly part: Extract<OwnerPart, { kind: "app" }>;
  /** Opens the app's move dialog, which says why it cannot move. */
  readonly onDetails: () => void;
  /** The checkboxes; an app is never checked unless the person checks it. */
  readonly check?: Check;
  readonly index: number;
  readonly active: boolean;
  readonly hint: string;
  readonly onOpen: () => void;
  readonly onMove: (event: KeyboardEvent, from: number) => void;
}) {
  const { m } = useI18n();
  const location = part.app.location ?? "";
  const state = check?.appState(part.app.key);
  return (
    <li>
      <div className="flex">
        <span className="flex w-7 shrink-0 justify-center pt-3">
          {check !== undefined && !part.app.protected && (
            <input
              type="checkbox"
              checked={state !== undefined && state.kind !== "refused"}
              disabled={state?.kind === "checking"}
              onChange={() => check.toggleApp(part.app.key)}
              aria-label={m.action.check(softwareTitle(part.app))}
              className="size-4 cursor-pointer accent-accent disabled:cursor-wait"
            />
          )}
        </span>
        <button
          type="button"
          data-row={index}
          aria-haspopup="dialog"
          tabIndex={active ? 0 : -1}
          aria-describedby={active ? hint : undefined}
          onClick={onOpen}
          onKeyDown={(event) => onMove(event, index)}
          className={`${GRID} min-w-0 flex-1 items-center gap-y-1 rounded px-2 py-2.5 text-left transition-colors hover:bg-fill focus-visible:bg-accent-soft focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-accent active:bg-fill-strong motion-reduce:transition-none`}
        >
          <span className="truncate border-l border-line-strong pl-3 font-medium text-ink">
            {m.results.apps.part}
          </span>
          <span className="col-start-2 row-start-1 justify-self-end text-right whitespace-nowrap tabular-nums text-ink @xl:col-start-4">
            {part.bytes === null ? (
              m.results.apps.notMeasured
            ) : part.atLeast ? (
              m.details.atLeast(formatBytes(part.bytes))
            ) : (
              <Bytes value={part.bytes} />
            )}
          </span>
          <span
            title={location}
            className="row-start-2 truncate text-[12px] text-ink-3 @xl:col-start-2 @xl:row-start-1 @xl:text-[13px]"
          >
            {location}
          </span>
          <span className="col-start-2 row-start-2 text-right text-[12px] text-ink-2 @xl:col-start-3 @xl:row-start-1 @xl:text-left @xl:text-[13px]">
            {part.app.protected ? m.apps.protected : m.results.apps.title}
          </span>
        </button>
      </div>
      {state?.kind === "checking" && (
        <p className="mr-2 mb-1.5 ml-9 text-[12px] text-ink-3">{m.results.apps.checking}</p>
      )}
      {state?.kind === "refused" && (
        <p className="mr-2 mb-1.5 ml-9 text-[12px] text-attention">
          {m.results.apps.refused}{" "}
          <button type="button" onClick={onDetails} className="text-accent-ink hover:underline">
            {m.common.details}
          </button>
        </p>
      )}
    </li>
  );
}

/**
 * A package its manager installed (ADR-0060): what it is and where; it moves
 * only through its manager, so it has no checkbox and no row of the keyboard's.
 */
function PackageRow({
  item,
  onInfo,
}: {
  readonly item: SoftwareItemView;
  readonly onInfo?: (item: SoftwareItemView) => void;
}) {
  const { m } = useI18n();
  const details = [m.apps.origins[item.origin], item.version]
    .filter((part): part is string => part !== null)
    .join(" · ");
  return (
    <li className="flex">
      <span className="w-7 shrink-0" />
      <button
        type="button"
        aria-haspopup="dialog"
        disabled={onInfo === undefined}
        onClick={() => onInfo?.(item)}
        className={`${GRID} min-w-0 flex-1 items-center gap-y-1 rounded px-2 py-2.5 text-left hover:bg-fill disabled:hover:bg-transparent`}
      >
        <span className="truncate border-l border-line-strong pl-3 font-medium text-ink">
          {softwareTitle(item)}
        </span>
        <span className="col-start-2 row-start-1 justify-self-end text-right whitespace-nowrap tabular-nums text-ink-2 @xl:col-start-4">
          {item.size === null || item.size.kind === "not_scanned"
            ? ""
            : item.size.kind === "at_least"
              ? m.details.atLeast(formatBytes(item.size.bytes))
              : formatBytes(item.size.bytes)}
        </span>
        <span
          title={item.location ?? ""}
          className="row-start-2 truncate text-[12px] text-ink-3 @xl:col-start-2 @xl:row-start-1 @xl:text-[13px]"
        >
          {item.location ?? ""}
        </span>
        <span className="col-start-2 row-start-2 text-right text-[12px] text-ink-2 @xl:col-start-3 @xl:row-start-1 @xl:text-left @xl:text-[13px]">
          {details}
        </span>
      </button>
    </li>
  );
}

/** A row that folds owners away: the small ones, or the system's apps. */
function FoldRow({
  label,
  open,
  onToggle,
}: {
  readonly label: string;
  readonly open: boolean;
  readonly onToggle: () => void;
}) {
  return (
    <div className="border-b border-line">
      <button
        type="button"
        aria-expanded={open}
        onClick={onToggle}
        className="flex w-full items-center gap-2 py-3 pr-2 pl-9 text-left text-section text-ink hover:bg-fill"
      >
        <span
          aria-hidden="true"
          className={`text-ink-3 transition-transform ${open ? "rotate-90" : ""}`}
        >
          ›
        </span>
        {label}
      </button>
    </div>
  );
}

/** A part the keyboard moves between: the app itself or an item, not a package. */
function isRow(part: OwnerPart): boolean {
  return part.kind !== "package";
}

/**
 * The candidates and installed apps under what owns them (ADR-0069), in
 * aligned columns, or a note when the filter leaves none. It is one stop for
 * Tab: ↑ and ↓ (and Page Up, Page Down, Home, End) move between rows.
 */
export function CandidateList({
  owners,
  onOpen,
  onOpenApp,
  lens,
  moving = null,
  check,
  folded,
  onInfo,
}: {
  readonly owners: readonly Owner[];
  /** Opens what Lumen knows about an installed app or package. */
  readonly onInfo?: (item: SoftwareItemView) => void;
  /** Owners folded into their own rows at the end: the small ones and the system's apps. */
  readonly folded?: { readonly small: readonly Owner[]; readonly system: readonly Owner[] };
  /** The checkboxes; none when the results are read-only. */
  readonly check?: Check;
  readonly onOpen: (candidate: CandidateView) => void;
  /** Opens an app's own move dialog. */
  readonly onOpenApp: (app: SoftwareItemView) => void;
  /** The item being moved to Quarantine now, by ID; its row shows it. */
  readonly moving?: string | null;
  /** The open item's lens: it lowers what is unrelated to that item. */
  readonly lens: Lens | null;
}) {
  const { m } = useI18n();
  const hint = useId();
  const list = useRef<HTMLDivElement>(null);
  const [selected, setSelected] = useState(0);
  const [smallOpen, setSmallOpen] = useState(false);
  const [systemOpen, setSystemOpen] = useState(false);
  const small = folded?.small ?? [];
  const system = folded?.system ?? [];
  // Every owner drawn now, in reading order: the folded ones only while open.
  const drawn = [...owners, ...(smallOpen ? small : []), ...(systemOpen ? system : [])];
  const count = drawn.reduce((total, owner) => total + owner.parts.filter(isRow).length, 0);
  // A filter may have shortened the list since.
  const active = Math.min(selected, count - 1);

  if (owners.length === 0 && small.length === 0 && system.length === 0) {
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

  // Each row's place in reading order, across owners.
  const starts: number[] = [];
  let next = 0;
  for (const owner of drawn) {
    starts.push(next);
    next += owner.parts.filter(isRow).length;
  }
  const rows = (owner: Owner, start: number) => {
    const places = owner.parts.map((part, i) =>
      isRow(part) ? start + owner.parts.slice(0, i).filter(isRow).length : -1,
    );
    return (
      <ul className="divide-y divide-line">
        {owner.parts.map((part, i) => {
          const at = places[i] ?? -1;
          if (part.kind === "package") {
            return (
              <PackageRow
                key={part.package.key}
                item={part.package}
                {...(onInfo === undefined ? {} : { onInfo })}
              />
            );
          }
          return part.kind === "app" ? (
            <AppRow
              key={part.app.key}
              part={part}
              {...(check === undefined ? {} : { check })}
              index={at}
              active={at === active}
              hint={hint}
              onOpen={() => {
                setSelected(at);
                (onInfo ?? onOpenApp)(part.app);
              }}
              onDetails={() => onOpenApp(part.app)}
              onMove={onMove}
            />
          ) : (
            <Row
              key={part.item.path}
              candidate={part.item}
              index={at}
              active={at === active}
              mark={lensMark(lens, part.item.path)}
              hint={hint}
              moving={part.item.id === moving}
              {...(check === undefined ? {} : { check })}
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
  };
  const section = (owner: Owner) => (
    <OwnerSection key={owner.key} owner={owner}>
      {rows(owner, starts[drawn.indexOf(owner)] ?? 0)}
    </OwnerSection>
  );

  return (
    <div ref={list} className="@container relative">
      <p id={hint} className="sr-only">
        {m.results.listHint}
      </p>
      <div
        aria-hidden="true"
        className={`${GRID} hidden border-b border-line pr-2 pb-1 pl-9 text-[12px] font-medium text-ink-3 @xl:grid`}
      >
        <span>{m.results.columns.name}</span>
        <span>{m.results.columns.location}</span>
        <span>{m.results.columns.status}</span>
        <span className="text-right">{m.results.columns.size}</span>
      </div>
      {owners.map(section)}
      {small.length > 0 && (
        <FoldRow
          label={m.results.apps.smallGroup(small.length)}
          open={smallOpen}
          onToggle={() => setSmallOpen(!smallOpen)}
        />
      )}
      {smallOpen && small.map(section)}
      {system.length > 0 && (
        <FoldRow
          label={m.results.apps.systemGroup(system.length)}
          open={systemOpen}
          onToggle={() => setSystemOpen(!systemOpen)}
        />
      )}
      {systemOpen && system.map(section)}
    </div>
  );
}

/** One owner: its name with its total and what is proposed, then its parts. */
function OwnerSection({
  owner,
  children,
}: {
  readonly owner: Owner;
  readonly children: ReactNode;
}) {
  const { m } = useI18n();
  const heading = useId();
  const name =
    owner.app !== null
      ? softwareTitle(owner.app)
      : owner.kind === "removed"
        ? m.results.groups.removed_app
        : owner.kind === "kind"
          ? m.results.groups[owner.group ?? "other"]
          : owner.kind === "packages" && owner.manager !== null
            ? m.apps.origins[owner.manager]
            : (owner.name ?? m.results.groups.other);
  const proposed = BigInt(owner.proposedBytes) > 0n;
  return (
    <section aria-labelledby={heading} className="border-b border-line pb-1">
      <ListHeading
        level={3}
        id={heading}
        name={name}
        count={owner.parts.length}
        bytes={owner.bytes}
        exact={!owner.atLeast}
        {...(proposed ? { detail: m.results.apps.proposed(formatBytes(owner.proposedBytes)) } : {})}
      />
      {owner.kind === "removed" && (
        <p className="pr-2 pb-1.5 pl-9 text-[12px] leading-snug text-ink-3">
          {m.results.removedNote}
        </p>
      )}
      {children}
    </section>
  );
}
