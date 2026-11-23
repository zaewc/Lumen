import type {
  CandidateView,
  GroupKindView,
  SoftwareItemView,
  SoftwareOriginView,
} from "@lumen/api-client";
import { largestFirst } from "@/entities/scan-session";
import { sumBytes } from "@/shared/lib";

/** One thing under an owner: the app itself, or one of the policy's items. */
export type OwnerPart =
  | {
      readonly kind: "app";
      readonly app: SoftwareItemView;
      /** Decimal bytes from the scan; `null` when the scan did not measure it. */
      readonly bytes: string | null;
      /** Some folder in it could not be read: `bytes` is a lower bound. */
      readonly atLeast: boolean;
    }
  | { readonly kind: "item"; readonly item: CandidateView }
  /** A package its manager installed: listed, moved only through that manager (ADR-0060). */
  | { readonly kind: "package"; readonly package: SoftwareItemView };

/**
 * Something a person recognises, with everything of it the host reported
 * (ADR-0069): an installed app, a developer tool, the data of removed apps,
 * or, when nothing owns an item, its kind. Display only: an owner carries no
 * verdict, and nothing is decided or selected per owner.
 */
export interface Owner {
  /** Stable across renders: `app:<id>`, `tool:<name>`, `source:<id>`, `removed`, `kind:<kind>`. */
  readonly key: string;
  readonly kind: "app" | "tool" | "source" | "removed" | "kind" | "packages";
  /** The app's or tool's name, or the owning ID; `null` for `removed` and `kind`. */
  readonly name: string | null;
  /** For `kind` owners, the kind of their items; otherwise `null`. */
  readonly group: GroupKindView | null;
  /** For `packages` owners, the manager (Homebrew counts its casks too); otherwise `null`. */
  readonly manager: SoftwareOriginView | null;
  /** The installed app this owner is, if any. */
  readonly app: SoftwareItemView | null;
  /** The app part first, then items largest first. */
  readonly parts: readonly OwnerPart[];
  /** Decimal bytes of the parts with a known size; `null` when none has one. */
  readonly bytes: string | null;
  /** Some part's size is unknown or a lower bound, so `bytes` is one too. */
  readonly atLeast: boolean;
  /** What the policy proposes among the parts, decimal bytes. */
  readonly proposedBytes: string;
}

/** A package's manager: Homebrew holds its formulas and casks together. */
function managerOf(item: SoftwareItemView): SoftwareOriginView {
  return item.origin === "homebrew_cask" ? "homebrew" : item.origin;
}

/** What a package or app measures in the latest scan, as an owner sums it. */
function measured(item: SoftwareItemView): { bytes: string | null; atLeast: boolean } {
  const size = item.size;
  const known = size !== null && size.kind !== "not_scanned";
  return { bytes: known ? size.bytes : null, atLeast: known && size.kind === "at_least" };
}

function appPart(app: SoftwareItemView): OwnerPart {
  return { kind: "app", app, ...measured(app) };
}

/**
 * Where an item belongs: the installed app it names (by bundle ID, or a
 * tool by the app's name), the tool, the removed apps, or its kind.
 */
function ownerKey(
  item: CandidateView,
  byId: ReadonlyMap<string, SoftwareItemView>,
  byName: ReadonlyMap<string, SoftwareItemView>,
  managers: ReadonlySet<SoftwareOriginView>,
): string {
  const { kind, source } = item.group;
  if (kind === "removed_app") return "removed";
  if (source === null) return `kind:${kind}`;
  const app =
    byId.get(source) ?? (kind === "developer" ? byName.get(source.toLowerCase()) : undefined);
  if (app !== undefined) return `app:${app.id}`;
  // A package manager's own cache sits with its packages.
  if (kind === "developer" && source.toLowerCase() === "homebrew" && managers.has("homebrew")) {
    return "packages:homebrew";
  }
  return kind === "developer" ? `tool:${source}` : `source:${source}`;
}

/** One removed app's data under the removed apps' owner (ADR-0068). */
export interface RemovedApp {
  /** The app's bundle ID as the item's name spells it; `null` when the host gave none. */
  readonly id: string | null;
  /** The app's name when Lumen removed it (design Q3); `null` otherwise. */
  readonly name: string | null;
  /** Lumen still holds the app in Quarantine: restoring it uses this data. */
  readonly held: boolean;
  /** Its parts are the owner's parts from `from` up to, not including, `to`. */
  readonly from: number;
  readonly to: number;
  /** Decimal bytes of its items. */
  readonly bytes: string;
  /** Some item's size is a lower bound, so `bytes` is one too. */
  readonly atLeast: boolean;
}

/** Items of removed apps, app by app: largest app first, each largest first. */
function byRemovedApp(items: readonly CandidateView[]): CandidateView[] {
  const apps = new Map<string | null, CandidateView[]>();
  for (const item of items) {
    apps.set(item.group.source, [...(apps.get(item.group.source) ?? []), item]);
  }
  const total = (members: readonly CandidateView[]) =>
    BigInt(sumBytes(members.map((item) => item.reclaimable_bytes)));
  return [...apps]
    .toSorted(([a, x], [b, y]) => {
      const [p, q] = [total(x), total(y)];
      return p !== q ? (p > q ? -1 : 1) : (a ?? "").localeCompare(b ?? "");
    })
    .flatMap(([, members]) => members);
}

/**
 * The removed apps of the removed apps' owner, in the order of its parts:
 * each app's items are next to each other (`ownersOf` puts them so, and a
 * filter keeps the order). Any other owner has none.
 */
export function removedApps(owner: Owner): RemovedApp[] {
  if (owner.kind !== "removed") return [];
  const apps: RemovedApp[] = [];
  owner.parts.forEach((part, at) => {
    if (part.kind !== "item") return;
    const { source, removed_app: record } = part.item.group;
    const last = apps.at(-1);
    const bytes = part.item.reclaimable_bytes;
    if (last !== undefined && last.id === source) {
      apps[apps.length - 1] = {
        ...last,
        name: last.name ?? record?.name ?? null,
        held: last.held || record?.held === true,
        to: at + 1,
        bytes: sumBytes([last.bytes, bytes]),
        atLeast: last.atLeast || !part.item.reclaimable_exact,
      };
    } else {
      apps.push({
        id: source,
        name: record?.name ?? null,
        held: record?.held === true,
        from: at,
        to: at + 1,
        bytes,
        atLeast: !part.item.reclaimable_exact,
      });
    }
  });
  return apps;
}

function compareOwners(a: Owner, b: Owner): number {
  const [x, y] = [BigInt(a.bytes ?? "0"), BigInt(b.bytes ?? "0")];
  if (x !== y) return x > y ? -1 : 1;
  return a.key.localeCompare(b.key);
}

/**
 * The owners of a scan's items and of everything installed, largest first
 * (ADR-0069): every candidate is under exactly one owner; every macOS app is
 * an owner with its app part (the system's included, which never move), so
 * it can be found and moved without a scan; every other package is under its
 * manager. Before any scan, `candidates` is empty.
 */
export function ownersOf(
  candidates: readonly CandidateView[],
  software: readonly SoftwareItemView[],
): Owner[] {
  const apps = software.filter((item) => item.origin === "mac_app");
  // What the person installed: not a package installed only for another, nor
  // one the platform's own list hides.
  const packages = software.filter(
    (item) => item.origin !== "mac_app" && item.explicit !== false && !item.hidden,
  );
  const managers = new Set(packages.map(managerOf));
  const byId = new Map(apps.map((app) => [app.id, app]));
  const byName = new Map(
    apps.flatMap((app) => (app.name === null ? [] : [[app.name.toLowerCase(), app] as const])),
  );
  const items = new Map<string, CandidateView[]>();
  for (const item of candidates) {
    const key = ownerKey(item, byId, byName, managers);
    items.set(key, [...(items.get(key) ?? []), item]);
  }
  const keys = new Set([
    ...apps.map((app) => `app:${app.id}`),
    ...[...managers].map((manager) => `packages:${manager}`),
    ...items.keys(),
  ]);
  return [...keys]
    .map((key): Owner => {
      const sorted = (items.get(key) ?? []).toSorted(largestFirst);
      const owned = key === "removed" ? byRemovedApp(sorted) : sorted;
      const app = key.startsWith("app:") ? (byId.get(key.slice(4)) ?? null) : null;
      const manager = key.startsWith("packages:")
        ? (key.slice("packages:".length) as SoftwareOriginView)
        : null;
      const parts: OwnerPart[] = [
        ...(app === null ? [] : [appPart(app)]),
        ...owned.map((item): OwnerPart => ({ kind: "item", item })),
        ...(manager === null
          ? []
          : packages
              .filter((each) => managerOf(each) === manager)
              .map((each): OwnerPart => ({ kind: "package", package: each }))),
      ];
      const sizes = parts.map((part) =>
        part.kind === "item"
          ? { bytes: part.item.reclaimable_bytes, atLeast: !part.item.reclaimable_exact }
          : part.kind === "app"
            ? { bytes: part.bytes, atLeast: part.atLeast }
            : measured(part.package),
      );
      const known = sizes.flatMap((size) => (size.bytes === null ? [] : [size.bytes]));
      const first = owned[0];
      const kind: Owner["kind"] =
        app !== null
          ? "app"
          : manager !== null
            ? "packages"
            : key === "removed"
              ? "removed"
              : key.startsWith("tool:")
                ? "tool"
                : key.startsWith("source:")
                  ? "source"
                  : "kind";
      return {
        key,
        kind,
        name:
          app !== null
            ? (app.name ?? app.id)
            : kind === "tool" || kind === "source"
              ? key.slice(key.indexOf(":") + 1)
              : null,
        group: kind === "kind" && first !== undefined ? first.group.kind : null,
        manager,
        app,
        parts,
        bytes: known.length === 0 ? null : sumBytes(known),
        atLeast: sizes.some((size) => size.bytes === null || size.atLeast),
        proposedBytes: sumBytes(
          owned
            .filter((item) => item.verdict === "quarantine")
            .map((item) => item.reclaimable_bytes),
        ),
      };
    })
    .sort(compareOwners);
}

/** The fewest bytes an owner with no item needs to be listed on its own: ADR-0039's floor. */
export const APP_FLOOR = 100_000_000n;

/** How the list is ordered. */
export type OwnerOrder = "size" | "name";

/** What the list shows of the owners (ADR-0069). */
export interface Listed {
  /** Listed on their own. */
  readonly owners: readonly Owner[];
  /** Apps and package managers with no item, below [`APP_FLOOR`] or unmeasured: one folded row. */
  readonly small: readonly Owner[];
  /** The system's own apps, which never move: one folded row. */
  readonly system: readonly Owner[];
}

/**
 * Everything an owner can be found by: its name, its apps' and packages'
 * names and IDs, its items' paths, and the names Lumen recorded for removed apps.
 */
function words(owner: Owner): string {
  return [
    owner.name,
    ...owner.parts.flatMap((part) =>
      part.kind === "item"
        ? [part.item.path, part.item.group.removed_app?.name ?? null]
        : part.kind === "app"
          ? [part.app.name, part.app.id, part.app.vendor]
          : [part.package.name, part.package.id, part.package.vendor],
    ),
  ]
    .filter((text): text is string => text !== null)
    .join(" ")
    .toLocaleLowerCase();
}

/**
 * The owners to list. With `apps` (the "All" view) apps and packages stay;
 * otherwise (a status filter) only items, and an owner left with nothing is
 * left out. A search keeps the owners that match every word, all listed on
 * their own; without one, owners with no item that are small or unmeasured,
 * and the system's apps, fold into their own rows.
 */
export function listedOwners(
  owners: readonly Owner[],
  view: { readonly apps: boolean; readonly query?: string; readonly order?: OwnerOrder },
): Listed {
  const query = (view.query ?? "").trim().toLocaleLowerCase().split(/\s+/u).filter(Boolean);
  const shown = owners.flatMap((owner): Owner[] => {
    const parts = view.apps ? owner.parts : owner.parts.filter((part) => part.kind === "item");
    if (parts.length === 0) return [];
    const kept = parts.length === owner.parts.length ? owner : { ...owner, parts };
    const text = words(kept);
    return query.every((word) => text.includes(word)) ? [kept] : [];
  });
  const sorted =
    view.order === "name"
      ? shown.toSorted((a, b) =>
          (a.name ?? a.key).localeCompare(b.name ?? b.key, undefined, { sensitivity: "base" }),
        )
      : shown;
  if (query.length > 0) return { owners: sorted, small: [], system: [] };
  const system = sorted.filter((owner) =>
    owner.parts.every((part) => part.kind === "app" && part.app.protected),
  );
  const small = sorted.filter(
    (owner) =>
      !system.includes(owner) &&
      owner.parts.every((part) => part.kind !== "item") &&
      (owner.bytes === null || BigInt(owner.bytes) < APP_FLOOR),
  );
  return {
    owners: sorted.filter((owner) => !system.includes(owner) && !small.includes(owner)),
    small,
    system,
  };
}
