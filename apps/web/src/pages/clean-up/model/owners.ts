import type { CandidateView, GroupKindView, SoftwareItemView } from "@lumen/api-client";
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
  | { readonly kind: "item"; readonly item: CandidateView };

/**
 * Something a person recognises, with everything of it the host reported
 * (ADR-0069): an installed app, a developer tool, the data of removed apps,
 * or, when nothing owns an item, its kind. Display only: an owner carries no
 * verdict, and nothing is decided or selected per owner.
 */
export interface Owner {
  /** Stable across renders: `app:<id>`, `tool:<name>`, `source:<id>`, `removed`, `kind:<kind>`. */
  readonly key: string;
  readonly kind: "app" | "tool" | "source" | "removed" | "kind";
  /** The app's or tool's name, or the owning ID; `null` for `removed` and `kind`. */
  readonly name: string | null;
  /** For `kind` owners, the kind of their items; otherwise `null`. */
  readonly group: GroupKindView | null;
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

/** A movable macOS app: never one of the system's (ADR-0060 rule 4). */
function isApp(item: SoftwareItemView): boolean {
  return item.origin === "mac_app" && !item.protected;
}

function appPart(app: SoftwareItemView): OwnerPart {
  const size = app.size;
  const measured = size !== null && size.kind !== "not_scanned";
  return {
    kind: "app",
    app,
    bytes: measured ? size.bytes : null,
    atLeast: measured && size.kind === "at_least",
  };
}

/**
 * Where an item belongs: the installed app it names (by bundle ID, or a
 * tool by the app's name), the tool, the removed apps, or its kind.
 */
function ownerKey(
  item: CandidateView,
  byId: ReadonlyMap<string, SoftwareItemView>,
  byName: ReadonlyMap<string, SoftwareItemView>,
): string {
  const { kind, source } = item.group;
  if (kind === "removed_app") return "removed";
  if (source === null) return `kind:${kind}`;
  const app =
    byId.get(source) ?? (kind === "developer" ? byName.get(source.toLowerCase()) : undefined);
  if (app !== undefined) return `app:${app.id}`;
  return kind === "developer" ? `tool:${source}` : `source:${source}`;
}

function compareOwners(a: Owner, b: Owner): number {
  const [x, y] = [BigInt(a.bytes ?? "0"), BigInt(b.bytes ?? "0")];
  if (x !== y) return x > y ? -1 : 1;
  return a.key.localeCompare(b.key);
}

/**
 * The owners of a scan's items and of the installed apps, largest first.
 * Every candidate is under exactly one owner; every movable macOS app is an
 * owner with its app part, even with nothing else under it, so it can be
 * moved without a scan. Before any scan, `candidates` is empty.
 */
export function ownersOf(
  candidates: readonly CandidateView[],
  software: readonly SoftwareItemView[],
): Owner[] {
  const apps = software.filter((item) => item.origin === "mac_app");
  const byId = new Map(apps.map((app) => [app.id, app]));
  const byName = new Map(
    apps.flatMap((app) => (app.name === null ? [] : [[app.name.toLowerCase(), app] as const])),
  );
  const items = new Map<string, CandidateView[]>();
  for (const item of candidates) {
    const key = ownerKey(item, byId, byName);
    items.set(key, [...(items.get(key) ?? []), item]);
  }
  const keys = new Set([...apps.filter(isApp).map((app) => `app:${app.id}`), ...items.keys()]);
  return [...keys]
    .map((key): Owner => {
      const owned = (items.get(key) ?? []).toSorted(largestFirst);
      const app = key.startsWith("app:") ? (byId.get(key.slice(4)) ?? null) : null;
      const parts: OwnerPart[] = [
        ...(app !== null && isApp(app) ? [appPart(app)] : []),
        ...owned.map((item): OwnerPart => ({ kind: "item", item })),
      ];
      const known = parts.flatMap((part) =>
        part.kind === "app"
          ? part.bytes === null
            ? []
            : [part.bytes]
          : [part.item.reclaimable_bytes],
      );
      const atLeast = parts.some((part) =>
        part.kind === "app" ? part.bytes === null || part.atLeast : !part.item.reclaimable_exact,
      );
      const first = owned[0];
      const kind: Owner["kind"] =
        app !== null
          ? "app"
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
        app,
        parts,
        bytes: known.length === 0 ? null : sumBytes(known),
        atLeast,
        proposedBytes: sumBytes(
          owned
            .filter((item) => item.verdict === "quarantine")
            .map((item) => item.reclaimable_bytes),
        ),
      };
    })
    .sort(compareOwners);
}

/** The fewest bytes an app with nothing else listed needs: ADR-0039's floor for large items. */
export const APP_FLOOR = 100_000_000n;

/** What the list shows of the owners, and the movable apps it leaves out. */
export interface Listed {
  readonly owners: readonly Owner[];
  /** Apps the scan did not measure: never given a made-up size or row. */
  readonly unmeasured: number;
}

/**
 * The owners to list. With `apps` (the "All" view), app parts stay; an app
 * with nothing else is listed only from [`APP_FLOOR`] and only when the scan
 * measured it. Otherwise (a status filter) only items are listed, and an
 * owner left with nothing is left out.
 */
export function listedOwners(owners: readonly Owner[], apps: boolean): Listed {
  let unmeasured = 0;
  const listed = owners.flatMap((owner): Owner[] => {
    const parts = apps ? owner.parts : owner.parts.filter((part) => part.kind === "item");
    if (parts.length === 0) return [];
    const [only] = parts;
    if (parts.length === 1 && only?.kind === "app") {
      if (only.bytes === null) {
        unmeasured += 1;
        return [];
      }
      if (BigInt(only.bytes) < APP_FLOOR) return [];
    }
    return [parts.length === owner.parts.length ? owner : { ...owner, parts }];
  });
  return { owners: listed, unmeasured };
}
