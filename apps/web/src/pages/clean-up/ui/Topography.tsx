import type { CandidateView } from "@lumen/api-client";
import { type KeyboardEvent, useLayoutEffect, useRef, useState } from "react";
import { useI18n } from "@/shared/i18n";
import { baseName, formatBytes } from "@/shared/lib";
import { type Filter, shows } from "../model/filter";
import { type Lens, lensMark } from "../model/lens";
import {
  appIconLabel,
  heightFor,
  labelFor,
  layoutTopography,
  type Placed,
  showAppIcon,
  type TopoGroup,
  type TopoNode,
  trailTo,
} from "../model/topography";

/** Room a group keeps for its name above its regions, and around them. */
const HEADER = 22;
const GAP = 3;

function groupName(m: ReturnType<typeof useI18n>["m"], group: TopoGroup): string {
  if (group.id === group.kind) {
    return m.results.groups[group.kind];
  }
  return group.source ?? m.results.otherTools;
}

function AppIcon({ source }: { readonly source: string | null }) {
  const label = appIconLabel(source);
  if (!label) {
    return null;
  }
  return (
    <span className="topo-app-icon" aria-hidden="true" title={source ?? undefined}>
      {label}
    </span>
  );
}

/**
 * The storage topography (docs/design/visual-language.md): every assessed
 * item as a region sized by what it would free, grouped by kind and tool.
 * Selecting a group zooms into it and narrows the list to it; selecting an
 * item opens its details. The status filter lowers the items outside it,
 * and so does an open item's lens, outlining what is related to it.
 * Items are reached by keyboard in the list below, so only groups and the
 * breadcrumb are tab stops here.
 */
export function Topography({
  groups,
  unread,
  zoom,
  onZoom,
  filter,
  lens,
  onOpen,
}: {
  readonly groups: readonly TopoGroup[];
  /** Entries the scan could not read; drawn at the top level only. */
  readonly unread: bigint;
  /** The group zoomed into; `null` for all items. */
  readonly zoom: string | null;
  readonly onZoom: (group: string | null) => void;
  readonly filter: Filter;
  readonly lens: Lens | null;
  readonly onOpen: (candidate: CandidateView) => void;
}) {
  const { m } = useI18n();
  const frame = useRef<HTMLDivElement>(null);
  const [width, setWidth] = useState(0);

  useLayoutEffect(() => {
    const element = frame.current;
    if (element === null) {
      return;
    }
    const observer = new ResizeObserver(([entry]) => {
      setWidth(Math.floor(entry?.contentRect.width ?? 0));
    });
    observer.observe(element);
    return () => observer.disconnect();
  }, []);

  const trail = zoom === null ? [] : (trailTo(groups, zoom) ?? []);
  const current = trail.at(-1);
  const nodes: readonly TopoNode[] = current?.children ?? groups;
  const height = heightFor(width);
  const placed =
    width === 0
      ? []
      : layoutTopography(nodes, { x: 0, y: 0, width, height }, current ? 0n : unread, {
          header: HEADER,
          gap: GAP,
        });

  // The region that had focus goes away when the map zooms; the frame keeps it.
  const zoomTo = (group: string | null) => {
    onZoom(group);
    frame.current?.focus();
  };
  const back = (event: KeyboardEvent) => {
    if (event.key === "Escape" && trail.length > 0) {
      event.stopPropagation();
      zoomTo(trail.at(-2)?.id ?? null);
    }
  };

  return (
    <section className="space-y-2">
      <nav aria-label={m.results.map.label}>
        <ol className="flex flex-wrap items-center gap-x-1.5 text-[13px] text-ink-2">
          {[null, ...trail].map((group, n) => {
            const last = n === trail.length;
            const name = group === null ? m.results.map.all : groupName(m, group);
            return (
              <li key={group?.id ?? ""} className="flex items-center gap-x-1.5">
                {n > 0 && (
                  <span aria-hidden="true" className="text-ink-3">
                    /
                  </span>
                )}
                {last ? (
                  <span aria-current="location" className="font-medium text-ink">
                    {name}
                  </span>
                ) : (
                  <button
                    type="button"
                    onClick={() => zoomTo(group?.id ?? null)}
                    className="rounded hover:text-ink hover:underline"
                  >
                    {name}
                  </button>
                )}
              </li>
            );
          })}
        </ol>
      </nav>
      {/* biome-ignore lint/a11y/noStaticElementInteractions: Escape steps out of a zoomed group; the regions inside are the controls. */}
      <div
        ref={frame}
        tabIndex={-1}
        onKeyDown={back}
        className="topo focus:outline-none"
        style={{ height }}
      >
        {placed.map((p) => (
          <Region
            key={regionKey(p)}
            placed={p}
            filter={filter}
            lens={lens}
            onZoom={zoomTo}
            onOpen={onOpen}
          />
        ))}
      </div>
    </section>
  );
}

function regionKey(placed: Placed): string {
  return placed.region.type === "node" ? placed.region.node.id : placed.region.id;
}

function Region({
  placed,
  filter,
  lens,
  onZoom,
  onOpen,
}: {
  readonly placed: Placed;
  readonly filter: Filter;
  readonly lens: Lens | null;
  readonly onZoom: (group: string) => void;
  readonly onOpen: (candidate: CandidateView) => void;
}) {
  const { m } = useI18n();
  const { region, rect } = placed;
  const box = {
    left: rect.x + 1,
    top: rect.y + 1,
    width: Math.max(rect.width - 2, 0),
    height: Math.max(rect.height - 2, 0),
  };
  const label = labelFor(box.width, box.height);

  if (region.type === "unread") {
    return (
      <div className="topo-region topo-unread" style={box}>
        {label !== "none" && (
          <span className="topo-label">
            <span className="truncate">{m.results.map.notRead}</span>
            {label === "full" && <span>{m.results.map.entries(Number(region.entries))}</span>}
          </span>
        )}
      </div>
    );
  }

  if (region.type === "smaller") {
    const size = formatBytes(region.bytes.toString());
    return (
      <div
        className="topo-region topo-smaller"
        style={box}
        title={`${m.results.map.smaller(region.count)} · ${size}`}
      >
        {label === "full" && (
          <span className="topo-label">
            <span className="truncate">{m.results.map.smaller(region.count)}</span>
            <span>{size}</span>
          </span>
        )}
      </div>
    );
  }

  const node = region.node;
  const size = formatBytes(node.bytes.toString());
  if (node.type === "group") {
    const name = groupName(m, node);
    return (
      <button
        type="button"
        onClick={() => onZoom(node.id)}
        aria-label={`${name}, ${size}, ${m.common.items(node.count)}`}
        className="topo-region topo-group"
        style={{ left: rect.x, top: rect.y, width: rect.width, height: rect.height }}
      >
        {rect.width >= 60 && (
          <span className="topo-group-label">
            <span className="truncate">{name}</span>
            {rect.width >= 140 && <span className="text-ink-3">{size}</span>}
          </span>
        )}
      </button>
    );
  }

  const name = baseName(node.candidate.path);
  const mark = lensMark(lens, node.candidate.path);
  const lowered = !shows(filter, node.candidate) || mark === "lowered";
  const canShowIcon = label === "full" && showAppIcon(box.width, box.height);
  return (
    <button
      type="button"
      tabIndex={-1}
      onClick={() => onOpen(node.candidate)}
      aria-label={`${name}, ${size}, ${m.results.status[node.status]}`}
      title={`${name} · ${size}`}
      data-status={node.status}
      data-lowered={lowered}
      data-inspected={mark === "inspected"}
      data-related={mark === "related"}
      className="topo-region topo-item"
      style={box}
    >
      {label !== "none" && (
        <span className="topo-label">
          {label === "full" && (
            <span className="topo-label-name">
              {canShowIcon && <AppIcon source={node.candidate.group.source} />}
              <span className="truncate">{name}</span>
            </span>
          )}
          <span>{size}</span>
        </span>
      )}
    </button>
  );
}
