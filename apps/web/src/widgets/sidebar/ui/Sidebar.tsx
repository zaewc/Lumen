import { Link, useNavigate } from "@tanstack/react-router";
import { type MouseEvent, useState } from "react";
import {
  dismissedUpdate,
  storeDismissedUpdate,
  useInstallUpdate,
  useUpdate,
} from "@/entities/app-update";
import { inQuarantine, useHeld } from "@/entities/held";
import { type TitleBar, useTransport } from "@/shared/api";
import { countFormat, useI18n } from "@/shared/i18n";
import { shortcutProps, useShortcut } from "@/shared/lib";
import { buttonClass, LogoMark } from "@/shared/ui";

/** The places of the app, as the navigation lists them. */
type Place = "/" | "/quarantine" | "/settings";

/**
 * A place's text label. The current place is ink with a cobalt rule under its
 * label; hovering shows the rule in a neutral tone (ADR-0052).
 */
const ITEM =
  "relative flex h-12 items-center justify-center gap-2 px-3 text-[13px] font-medium whitespace-nowrap text-ink-2 transition-colors hover:text-ink focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-accent aria-[current=page]:text-ink after:absolute after:inset-x-3 after:bottom-0 after:h-0.5 after:bg-transparent hover:after:bg-line-strong aria-[current=page]:after:bg-accent motion-reduce:transition-none min-[960px]:h-14";

/** The app's places, in the sidebar's order; ⌘1–⌘5 (Ctrl+1–5) open them. */
const PLACES: readonly Place[] = ["/", "/quarantine", "/settings"];

/** The shortcut key of a place: its position in the sidebar. */
function placeKey(place: Place): string {
  return String(PLACES.indexOf(place) + 1);
}

/** Opens the place on its shortcut. */
function usePlaceShortcut(place: Place): void {
  const navigate = useNavigate();
  useShortcut(placeKey(place), () => {
    void navigate({ to: place });
  });
}

/** One place in the app: its name, and an optional count. */
interface PlaceItem {
  readonly to: Place;
  readonly label: string;
  /** A count and what it means, for screen readers. */
  readonly badge?: { readonly text: string; readonly label: string };
}

/** A place's link. */
function NavItem({ to, label, badge }: PlaceItem) {
  return (
    <li className="min-w-0 flex-auto min-[960px]:flex-none">
      <Link
        to={to}
        activeOptions={{ exact: to === "/" }}
        {...shortcutProps(label, placeKey(to))}
        className={ITEM}
      >
        <span className="truncate">{label}</span>
        {/* Narrow navigation leaves the count out of sight but not out of reach. */}
        {badge !== undefined && (
          <span
            role="img"
            aria-label={badge.label}
            className="sr-only text-[12px] font-normal tabular-nums text-ink-3 min-[960px]:not-sr-only"
          >
            {badge.text}
          </span>
        )}
      </Link>
    </li>
  );
}

/** What a press on the header does where it is the title bar. */
function titleBarProps(bar: TitleBar | undefined) {
  if (bar === undefined) {
    return {};
  }
  return {
    onMouseDown: (event: MouseEvent<HTMLElement>) => {
      // Links and buttons keep their own presses.
      if (
        event.button !== 0 ||
        !(event.target instanceof Element) ||
        event.target.closest("a, button, input, select, textarea") !== null
      ) {
        return;
      }
      // A window that did not move or zoom is only that; there is nothing to tell.
      (event.detail === 2 ? bar.zoom() : bar.drag()).catch(() => undefined);
    },
  };
}

/**
 * The workspace navigation. On narrow windows the labelled places form a
 * second row; none of the destinations disappears into an icon. It names
 * places only: no status sits in it.
 */
export function Sidebar() {
  const { locale, m } = useI18n();
  const held = useHeld();
  const install = useInstallUpdate();
  const update = useUpdate();
  const version = update.data?.available ?? null;
  const [dismissed, dismiss] = useState(dismissedUpdate);
  const showUpdate = version !== null && version !== dismissed && install.status !== "pending";
  const titleBar = useTransport().titleBar;
  const count = inQuarantine(held.data ?? []).length;
  usePlaceShortcut("/");
  usePlaceShortcut("/quarantine");
  usePlaceShortcut("/settings");
  const places: readonly PlaceItem[] = [
    { to: "/", label: m.nav.overview },
    {
      to: "/quarantine",
      label: m.nav.quarantine,
      ...(count > 0
        ? { badge: { text: countFormat(locale)(count), label: m.nav.held(count) } }
        : {}),
    },
    { to: "/settings", label: m.nav.settings },
  ];
  return (
    <>
      <header
        {...titleBarProps(titleBar)}
        className={`flex shrink-0 flex-wrap items-center border-b border-line bg-plane px-3 min-[960px]:gap-4 min-[960px]:px-6 ${titleBar === undefined ? "" : "select-none"}`}
      >
        {/* Room for the window buttons the host draws here (lib.rs, TRAFFIC_LIGHTS). */}
        {titleBar !== undefined && (
          <div aria-hidden="true" className="h-14 w-[62px] shrink-0 min-[960px]:w-[50px]" />
        )}
        <div className="flex h-14 shrink-0 items-center gap-2.5 px-2 min-[960px]:pr-6">
          <LogoMark className="size-8" />
          <span className="text-[17px] font-semibold tracking-[-0.03em] text-ink">Lumen</span>
        </div>
        <nav
          aria-label={m.nav.label}
          className="order-last w-full min-[960px]:order-none min-[960px]:w-auto"
        >
          <ul className="flex min-w-0">
            {places.map((place) => (
              <NavItem key={place.to} {...place} />
            ))}
          </ul>
        </nav>
      </header>
      {showUpdate && (
        <aside
          aria-label={m.settings.about.update.title}
          className="flex shrink-0 flex-wrap items-center justify-between gap-x-6 gap-y-3 border-b border-accent/20 bg-accent-soft px-5 py-3 min-[960px]:px-8"
        >
          <p role="status" className="min-w-0 text-[14px] font-medium text-ink">
            {m.settings.about.update.notice(version)}
          </p>
          <div className="ml-auto flex shrink-0 items-center gap-2">
            <button
              type="button"
              className={buttonClass("quiet", "sm")}
              onClick={() => {
                storeDismissedUpdate(version);
                dismiss(version);
              }}
            >
              {m.settings.about.update.later}
            </button>
            <Link to="/settings" hash="app-update" className={buttonClass("primary", "sm")}>
              {m.settings.about.update.noticeAction}
            </Link>
          </div>
        </aside>
      )}
    </>
  );
}
