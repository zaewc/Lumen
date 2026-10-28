import { Link, useNavigate } from "@tanstack/react-router";
import type { ReactNode } from "react";
import { isOn, useAiSettings } from "@/entities/ai-review";
import { useUpdate } from "@/entities/app-update";
import { useHeld } from "@/entities/held";
import { countFormat, useI18n } from "@/shared/i18n";
import { shortcutProps, useShortcut } from "@/shared/lib";
import { ArchiveIcon, DriveIcon, GearIcon, LogoMark, SearchIcon } from "@/shared/ui";

const ITEM =
  "relative flex h-12 items-center justify-center gap-2 px-2 min-[960px]:px-3 text-[13px] font-medium whitespace-nowrap text-ink-2 transition-colors hover:bg-fill hover:text-ink focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-accent aria-[current=page]:text-ink after:absolute after:inset-x-3 after:bottom-0 after:h-0.5 after:bg-transparent aria-[current=page]:after:bg-accent motion-reduce:transition-none min-[960px]:h-[72px] [&>svg]:hidden min-[960px]:[&>svg]:block [&>svg]:size-[18px] [&>svg]:text-ink-3 aria-[current=page]:[&>svg]:text-accent-ink";

/** The app's places, in the sidebar's order; ⌘1–⌘4 (Ctrl+1–4) open them. */
const PLACES = ["/", "/clean", "/quarantine", "/settings"] as const;

type Place = (typeof PLACES)[number];

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

/** One place in the app: an icon, its name, and an optional count. */
function NavItem({
  to,
  icon,
  label,
  badge,
}: {
  readonly to: Place;
  readonly icon: ReactNode;
  readonly label: string;
  /** A count and what it means, for screen readers. */
  readonly badge?: { readonly text: string; readonly label: string };
}) {
  return (
    <li className="min-w-0 flex-1 min-[960px]:flex-none">
      <Link
        to={to}
        activeOptions={{ exact: to === "/" }}
        {...shortcutProps(label, placeKey(to))}
        className={ITEM}
      >
        {icon}
        <span className="truncate">{label}</span>
        {badge !== undefined && (
          <span
            role="img"
            aria-label={badge.label}
            className="hidden text-[11px] font-normal tabular-nums text-ink-3 min-[960px]:inline"
          >
            {badge.text}
          </span>
        )}
      </Link>
    </li>
  );
}

/** A newer version, when one is out; it opens Settings, where it installs. */
function UpdateStatus() {
  const { m } = useI18n();
  const update = useUpdate();
  const version = update.data?.available ?? null;
  if (version === null) {
    return null;
  }
  const label = m.settings.about.update.sidebar(version);
  return (
    <Link
      to="/settings"
      title={label}
      className="flex h-8 items-center gap-2 rounded px-2 text-[12px] font-medium text-accent-ink hover:bg-fill focus-visible:outline-2 focus-visible:outline-accent"
    >
      <span aria-hidden="true" className="size-2 shrink-0 rounded-full bg-accent" />
      <span className="truncate">{label}</span>
    </Link>
  );
}

/** Whether AI review is on, at the foot of the sidebar; it opens its setting. */
function AiStatus() {
  const { m } = useI18n();
  const settings = useAiSettings();
  if (!settings.isSuccess) {
    return null;
  }
  const on = isOn(settings.data) !== null;
  const label = on ? m.ai.short.on : m.ai.short.off;
  return (
    <Link
      to="/settings"
      hash="ai-review"
      activeOptions={{ includeHash: true }}
      title={label}
      className="flex h-8 items-center gap-2 rounded px-2 text-[12px] text-ink-2 hover:bg-fill hover:text-ink focus-visible:outline-2 focus-visible:outline-accent"
    >
      <span
        aria-hidden="true"
        className={`size-1.5 rounded-full ${on ? "bg-accent" : "bg-ink-3"}`}
      />
      <span className="truncate">{label}</span>
    </Link>
  );
}

/**
 * The workspace navigation. On narrow windows the four labelled places
 * form a second row; none of the destinations disappears into an icon.
 */
export function Sidebar() {
  const { locale, m } = useI18n();
  const held = useHeld();
  const count = held.data?.length ?? 0;
  usePlaceShortcut("/");
  usePlaceShortcut("/clean");
  usePlaceShortcut("/quarantine");
  usePlaceShortcut("/settings");
  return (
    <header className="flex shrink-0 flex-wrap items-center border-b border-line bg-sidebar px-3 min-[960px]:gap-4 min-[960px]:px-6">
      <div className="flex h-14 shrink-0 items-center gap-2.5 px-2 min-[960px]:h-[72px] min-[960px]:pr-6">
        <LogoMark className="size-8" />
        <span className="text-[21px] font-semibold tracking-[-0.06em] text-ink">Lumen</span>
      </div>
      <nav
        aria-label={m.nav.label}
        className="order-last w-full min-[960px]:order-none min-[960px]:w-auto"
      >
        <ul className="flex min-w-0">
          <NavItem to="/" icon={<DriveIcon />} label={m.nav.overview} />
          <NavItem to="/clean" icon={<SearchIcon />} label={m.nav.cleanUp} />
          <NavItem
            to="/quarantine"
            icon={<ArchiveIcon />}
            label={m.nav.quarantine}
            {...(count > 0
              ? { badge: { text: countFormat(locale)(count), label: m.nav.held(count) } }
              : {})}
          />
          <NavItem to="/settings" icon={<GearIcon />} label={m.nav.settings} />
        </ul>
      </nav>
      <div className="ml-auto flex min-w-0 flex-wrap justify-end gap-x-2">
        <UpdateStatus />
        <AiStatus />
      </div>
    </header>
  );
}
