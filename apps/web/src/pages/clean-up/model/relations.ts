import type {
  SoftwareItemView,
  SoftwareRelationMethodView,
  SoftwareRelationView,
} from "@lumen/api-client";
import type { Messages } from "@/shared/i18n";

/** The record a relation came from, as the details sheet names it. */
export type RelationRecord = keyof Messages["apps"]["relations"]["records"];

/** How sure a relation is: proven, one of several candidates, or neither. */
export type RelationCertainty = "verified" | "candidate" | "unconfirmed";

/** One entry of a details section. */
export interface RelationEntry {
  /** The other side as its record names it (`a | b` for alternatives). */
  readonly name: string;
  readonly record: RelationRecord;
  readonly certainty: RelationCertainty;
}

/**
 * One section of the details sheet: its entries, or `unread` where the
 * records were not read (never shown as none, ADR-0008). A section with no
 * entries and nothing unknown is `null` and not shown.
 */
export type RelationSection =
  | {
      readonly kind: "entries";
      readonly entries: readonly RelationEntry[];
      readonly atLeast: boolean;
    }
  | { readonly kind: "unread" };

/** What an item needs and what needs it, for its details sheet (ADR-0064). */
export interface ItemRelations {
  readonly needs: RelationSection | null;
  readonly neededBy: RelationSection | null;
}

export function relationRecord(method: SoftwareRelationMethodView): RelationRecord {
  switch (method) {
    case "homebrew_runtime_dependencies":
    case "cask_uninstall_artifacts":
    case "cask_definition":
    case "cask_default_app_folder":
      return "homebrew";
    case "receipt_file_list":
      return "receipt";
    case "dpkg_status":
      return "dpkg";
    case "pacman_local_database":
      return "pacman";
    case "rpm_database":
      return "rpm";
    case "other":
      return "other";
  }
}

function certainty(relation: SoftwareRelationView): RelationCertainty {
  if (relation.verified) return "verified";
  return relation.ambiguous ? "candidate" : "unconfirmed";
}

/**
 * One entry per name and record. A need several packages could meet comes
 * from the API once per candidate, all with the same recorded name; the
 * sheet shows it once.
 */
function entries(
  item: SoftwareItemView,
  kind: SoftwareRelationView["kind"],
): readonly RelationEntry[] {
  const seen = new Map<string, RelationEntry>();
  for (const relation of item.relations) {
    if (relation.kind !== kind) continue;
    const entry: RelationEntry = {
      name: relation.label,
      record: relationRecord(relation.method),
      certainty: certainty(relation),
    };
    const key = `${entry.record}\u0000${entry.name}`;
    const known = seen.get(key);
    // A name both proven and a candidate is shown as a candidate.
    if (known === undefined || (known.certainty === "verified" && entry.certainty !== "verified")) {
      seen.set(key, entry);
    }
  }
  return [...seen.values()];
}

/**
 * The details sheet's two sections. What an item needs comes from its own
 * record: unread when that was not read. What needs it comes from every
 * other record: a lower bound ("at least") unless Lumen read them all, and
 * unread when it found none and could not read them all.
 */
export function itemRelations(item: SoftwareItemView): ItemRelations {
  const needs = entries(item, "needs");
  const neededBy = entries(item, "needed_by");
  const allRead = item.facts.needed_by?.complete === true;
  // Only a package has a record of its own needs and dependents.
  const isPackage = item.relations_read !== null;
  return {
    needs:
      item.relations_read === false
        ? { kind: "unread" }
        : needs.length > 0
          ? { kind: "entries", entries: needs, atLeast: false }
          : null,
    neededBy:
      neededBy.length > 0
        ? { kind: "entries", entries: neededBy, atLeast: !allRead }
        : isPackage && !allRead
          ? { kind: "unread" }
          : null,
  };
}
