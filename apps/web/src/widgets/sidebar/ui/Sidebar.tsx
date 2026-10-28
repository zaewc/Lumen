import { Link, useNavigate } from "@tanstack/react-router";
import type { ReactNode } from "react";
import { isOn, useAiSettings } from "@/entities/ai-review";
import { useUpdate } from "@/entities/app-update";
import { useHeld } from "@/entities/held";
import { countFormat, useI18n } from "@/shared/i18n";
import { shortcutProps, useShortcut } from "@/shared/lib";
import { ArchiveIcon, GaugeIcon, GearIcon, LogoMark, SparkleIcon } from "@/shared/ui";

// Below 720 px the sidebar is a rail of icons; the labels stay for screen readers.

const ITEM =
  "relative flex h-10 items-center justify-center gap-2.5 rounded-lg px-2.5 text-[13px] font-medium text-ink transition-colors hover:bg-fill-strong focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-accent aria-[current=page]:bg-accent aria-[current=page]:text-white motion-reduce:transition-none min-[720px]:h-8 min-[720px]:justify-start [&>svg]:text-accent-ink aria-[current=page]:[&>svg]:text-white";

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
    <li>
      <Link
        to={to}
        activeOptions={{ exact: to === "/" }}
        {...shortcutProps(label, placeKey(to))}
        className={ITEM}
      >
        {icon}
        <span className="sr-only min-[720px]:not-sr-only min-[720px]:flex-1 min-[720px]:truncate">
          {label}
        </span>
        {badge !== undefined && (
          <span
            role="img"
            aria-label={badge.label}
            className="absolute top-0.5 right-0.5 min-w-4 rounded-full bg-accent px-1 text-center text-[10px] leading-4 font-semibold text-white tabular-nums ring-2 ring-sidebar [[aria-current=page]>&]:bg-white [[aria-current=page]>&]:text-accent min-[720px]:static min-[720px]:ring-0 min-[720px]:bg-fill-strong min-[720px]:px-1.5 min-[720px]:text-[11px] min-[720px]:leading-[18px] min-[720px]:font-medium min-[720px]:text-ink-2 min-[720px]:[[aria-current=page]>&]:bg-white/25 min-[720px]:[[aria-current=page]>&]:text-white"
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
      className="flex h-9 items-center justify-center gap-2 rounded-lg px-2.5 text-[12px] font-medium text-accent-ink hover:bg-fill-strong focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-accent min-[720px]:justify-start"
    >
      <span aria-hidden="true" className="size-2 shrink-0 rounded-full bg-accent" />
      <span className="sr-only min-[720px]:not-sr-only min-[720px]:truncate">{label}</span>
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
      className="flex h-9 items-center justify-center gap-2 rounded-lg px-2.5 text-[12px] text-ink-2 hover:bg-fill-strong hover:text-ink focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-accent min-[720px]:justify-start"
    >
      <span className="relative inline-flex">
        <SparkleIcon className="size-[18px]" />
        <span
          aria-hidden="true"
          className={`absolute -right-0.5 -bottom-0.5 size-2 rounded-full ring-2 ring-sidebar ${on ? "bg-emerald-500" : "bg-ink-3"}`}
        />
      </span>
      <span className="sr-only min-[720px]:not-sr-only min-[720px]:truncate">{label}</span>
    </Link>
  );
}

/**
 * The app's sidebar, as in a macOS utility: the app's name, its four places
 * and, at the foot, whether AI review is on. Below 720 px it narrows to a
 * rail of icons, so the window stays usable at its 420 px minimum.
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
    <aside className="flex w-[60px] shrink-0 flex-col border-r border-line bg-sidebar min-[720px]:w-[220px]">
      <div className="flex h-14 shrink-0 items-center justify-center gap-2.5 px-4 min-[720px]:justify-start">
        <LogoMark className="size-7" />
        <span className="sr-only text-[15px] font-semibold tracking-tight text-ink min-[720px]:not-sr-only">
          Lumen
        </span>
      </div>
      <nav aria-label={m.nav.label} className="flex-1 overflow-y-auto px-2 py-1 min-[720px]:px-3">
        <ul className="space-y-1 min-[720px]:space-y-0.5">
          <NavItem to="/" icon={<GaugeIcon />} label={m.nav.overview} />
          <NavItem to="/clean" icon={<SparkleIcon />} label={m.nav.cleanUp} />
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
      <div className="shrink-0 border-t border-line px-2 py-2 min-[720px]:px-3">
        <UpdateStatus />
        <AiStatus />
      </div>
    </aside>
  );
}
