import { Link, useNavigate } from "@tanstack/react-router";
import { type ReactNode, useId } from "react";
import { heldTotal, useHeld } from "@/entities/held";
import { summarize, useScanSession } from "@/entities/scan-session";
import { countFormat, useI18n } from "@/shared/i18n";
import { formatBytes } from "@/shared/lib";
import {
  ArchiveIcon,
  buttonClass,
  CheckIcon,
  ChevronIcon,
  IconTile,
  SearchIcon,
  SparkleIcon,
  WarningIcon,
} from "@/shared/ui";

const TILE =
  "group flex min-w-0 flex-col rounded-xl border border-line bg-surface p-4 transition-colors hover:border-line-strong hover:bg-raised focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent motion-reduce:transition-none";

/** One figure of the last scan; the whole tile opens the page with the details. */
function Tile({
  icon,
  label,
  value,
  detail,
  children,
}: {
  readonly icon: ReactNode;
  readonly label: string;
  readonly value: string;
  readonly detail: string;
  /** The link that wraps it, given its classes and content. */
  readonly children: (className: string, content: ReactNode) => ReactNode;
}) {
  const content = (
    <>
      <span className="flex items-center gap-2.5">
        {icon}
        <span className="min-w-0 flex-1 truncate text-[13px] font-medium text-ink-2">{label}</span>
        <ChevronIcon className="text-ink-3 transition-transform group-hover:translate-x-0.5 motion-reduce:transition-none" />
      </span>
      <span className="mt-3 text-[22px] leading-tight font-semibold tracking-tight tabular-nums text-ink">
        {value}
      </span>
      <span className="mt-0.5 text-[12px] tabular-nums text-ink-3">{detail}</span>
    </>
  );
  return children(TILE, content);
}

/** Quarantine's figure: how many items it holds and what removing them would free. */
function QuarantineTile() {
  const { m } = useI18n();
  const held = useHeld();
  const items = held.data ?? [];
  const total = heldTotal(items);
  const size = formatBytes(total.bytes);
  return (
    <Tile
      icon={
        <IconTile tone="neutral">
          <ArchiveIcon />
        </IconTile>
      }
      label={m.overview.session.quarantine}
      value={held.isSuccess ? m.common.items(items.length) : "–"}
      detail={total.exact ? size : m.overview.session.atLeast(size)}
    >
      {(className, content) => (
        <Link to="/quarantine" className={className}>
          {content}
        </Link>
      )}
    </Tile>
  );
}

/** A scan running in this session, with a way to its live progress. */
function Running() {
  const { locale, m } = useI18n();
  const session = useScanSession();
  const count = countFormat(locale);
  return (
    <div className="rounded-xl border border-line bg-surface p-5" role="status">
      <div className="flex flex-wrap items-center gap-3">
        <IconTile>
          <SearchIcon />
        </IconTile>
        <div className="min-w-0 flex-1">
          <p className="text-[14px] font-semibold text-ink">
            {session.scanning ? m.overview.session.scanning : m.overview.session.deciding}
          </p>
          {session.progress !== null && (
            <p className="text-[12px] tabular-nums text-ink-2">
              {m.scanning.items}: {count(session.progress.entries)}
            </p>
          )}
        </div>
        <Link to="/clean" className={buttonClass("secondary")}>
          {m.overview.session.showProgress}
        </Link>
      </div>
      <div className="mt-4 h-1.5 overflow-hidden rounded-full bg-accent-soft">
        <div className="h-full w-2/5 animate-[indeterminate_1.6s_ease-in-out_infinite] rounded-full bg-accent motion-reduce:w-full motion-reduce:animate-none motion-reduce:opacity-50" />
      </div>
    </div>
  );
}

/** No results in this session yet: what Lumen does, and the button that starts it. */
function Empty() {
  const { m } = useI18n();
  const session = useScanSession();
  const navigate = useNavigate();
  const report = session.report;
  return (
    <div className="flex flex-col items-center rounded-xl border border-line bg-surface px-6 py-8 text-center">
      <span className="flex size-12 items-center justify-center rounded-2xl bg-accent-soft text-accent-ink">
        <SparkleIcon className="size-6" />
      </span>
      {report !== null && (
        <p className="mt-4 flex items-center gap-1.5 text-[13px] font-medium text-emerald-700 dark:text-emerald-400">
          <CheckIcon className="size-4" />
          {m.overview.session.cleaned(
            report.items.filter((item) => item.result.kind === "quarantined").length,
            formatBytes(report.moved_bytes),
          )}
        </p>
      )}
      <h3 className="mt-4 text-[17px] font-semibold tracking-tight text-ink">
        {m.overview.empty.title}
      </h3>
      <p className="mt-1.5 max-w-md text-[13px] leading-relaxed text-ink-2">
        {m.overview.empty.body}
      </p>
      {session.scanError !== null && (
        <p className="mt-3 flex items-center gap-1.5 text-[13px] text-amber-800 dark:text-amber-300">
          <WarningIcon className="size-4" />
          {m.start.failed}
        </p>
      )}
      <button
        type="button"
        onClick={() => {
          session.start([]);
          void navigate({ to: "/clean" });
        }}
        className={`${buttonClass("primary", "lg")} mt-6`}
      >
        <SearchIcon className="size-[18px]" />
        {m.overview.empty.scan}
      </button>
    </div>
  );
}

/**
 * The last scan of this session, in three tiles (what can be cleaned up,
 * what needs a look, what Quarantine holds), or the way to start one.
 */
export function SessionSummary() {
  const { m } = useI18n();
  const session = useScanSession();
  const held = useHeld();
  const title = useId();
  const running = session.scanning || (session.scan !== null && session.assessment === "pending");
  const summary = session.candidates === null ? null : summarize(session.candidates);
  return (
    <section aria-labelledby={title} className="space-y-3">
      <h2 id={title} className="px-1 text-[13px] font-semibold text-ink-2">
        {m.overview.session.title}
      </h2>
      {running && <Running />}
      {!running && summary === null && <Empty />}
      {summary !== null && (
        <div className="grid gap-3 @lg:grid-cols-3">
          <Tile
            icon={
              <IconTile>
                <SparkleIcon />
              </IconTile>
            }
            label={m.overview.session.canFree}
            value={formatBytes(summary.quarantine.bytes)}
            detail={m.common.items(summary.quarantine.items.length)}
          >
            {(className, content) => (
              <Link to="/clean" className={className}>
                {content}
              </Link>
            )}
          </Tile>
          <Tile
            icon={
              <IconTile tone="amber">
                <WarningIcon className="size-[18px]" />
              </IconTile>
            }
            label={m.overview.session.review}
            value={m.common.items(summary.review.items.length)}
            detail={formatBytes(summary.review.bytes)}
          >
            {(className, content) => (
              <Link to="/clean" className={className}>
                {content}
              </Link>
            )}
          </Tile>
          <QuarantineTile />
        </div>
      )}
      {summary === null && !running && (held.data?.length ?? 0) > 0 && (
        <div className="grid gap-3 @lg:grid-cols-3">
          <QuarantineTile />
        </div>
      )}
    </section>
  );
}
