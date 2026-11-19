import type { AppConfirmationRequest, CandidateView } from "@lumen/api-client";
import { statusOf } from "@/entities/scan-session";
import { canChooseReview } from "@/shared/api";
import { sumBytes } from "@/shared/lib";

/**
 * What a person changed from Lumen's proposal (ADR-0069): the proposals they
 * unchecked and the review items they checked, by ID. Everything else is as
 * proposed, so a new assessment starts from the proposal again.
 */
export interface Selection {
  readonly leftOut: ReadonlySet<string>;
  readonly chosen: ReadonlySet<string>;
  /** Checked apps by software-list key, each with its fresh preview (ADR-0062). */
  readonly apps: ReadonlyMap<string, AppCheck>;
  /**
   * Checked copies of identical files by [`copyKey`], each with its fresh
   * preview (ADR-0069). Lumen never checks one.
   */
  readonly copies: ReadonlyMap<string, CopyCheck>;
}

/** A checked copy: its preview's state, and the place of the copy it keeps. */
export interface CopyCheck {
  readonly check: AppCheck;
  readonly stays: number;
}

/** A copy's key: its comparison, its group's place and its own place. */
export function copyKey(comparison: string, group: number, copy: number): string {
  return `${comparison}:${group}:${copy}`;
}

/**
 * A checked app: its preview is being made, it is ready to move (by the
 * preview's ID and hash), or the host refused it. Only a ready app moves.
 */
export type AppCheck =
  | { readonly kind: "checking" }
  | {
      readonly kind: "ready";
      readonly preview: string;
      readonly hash: string;
      /** Decimal bytes, as previewed. */
      readonly bytes: string;
    }
  | { readonly kind: "refused" };

export const PROPOSED: Selection = {
  leftOut: new Set(),
  chosen: new Set(),
  apps: new Map(),
  copies: new Map(),
};

/** The selection with copy `key` set to `check`, or unchecked with `undefined`. */
export function withCopy(
  selection: Selection,
  key: string,
  check: CopyCheck | undefined,
): Selection {
  const copies = new Map(selection.copies);
  if (check === undefined) copies.delete(key);
  else copies.set(key, check);
  return { ...selection, copies };
}

/** The selection with app `key` set to `check`, or unchecked with `undefined`. */
export function withApp(selection: Selection, key: string, check: AppCheck | undefined): Selection {
  const apps = new Map(selection.apps);
  if (check === undefined) apps.delete(key);
  else apps.set(key, check);
  return { ...selection, apps };
}

/** A proposal: what a plan holds unless it is left out. */
function proposed(candidate: CandidateView): boolean {
  return statusOf(candidate) === "quarantine";
}

/** Whether the item has a checkbox: a proposal, or a review item a person may choose (ADR-0051). */
export function checkable(candidate: CandidateView): boolean {
  return proposed(candidate) || canChooseReview(candidate);
}

export function isChecked(selection: Selection, candidate: CandidateView): boolean {
  return proposed(candidate)
    ? !selection.leftOut.has(candidate.id)
    : canChooseReview(candidate) && selection.chosen.has(candidate.id);
}

function flip(set: ReadonlySet<string>, id: string): ReadonlySet<string> {
  const next = new Set(set);
  if (!next.delete(id)) next.add(id);
  return next;
}

/** The selection with `candidate` checked or unchecked; others unchanged. */
export function toggled(selection: Selection, candidate: CandidateView): Selection {
  if (proposed(candidate)) return { ...selection, leftOut: flip(selection.leftOut, candidate.id) };
  if (canChooseReview(candidate))
    return { ...selection, chosen: flip(selection.chosen, candidate.id) };
  return selection;
}

/** What would move, and what to ask the host to plan. */
export interface Selected {
  /** Items and ready apps. */
  readonly count: number;
  /** Items only: whether there is a clean-up plan to make. */
  readonly items: number;
  /** Decimal bytes. */
  readonly bytes: string;
  readonly chosen: readonly string[];
  readonly leftOut: readonly string[];
  /** Ready apps, to confirm with the plan. */
  readonly apps: readonly AppConfirmationRequest[];
  /** Ready copies, to confirm with the plan. */
  readonly copies: readonly AppConfirmationRequest[];
}

/** The checked items of `candidates`; IDs no longer in them are dropped. */
export function selectedOf(selection: Selection, candidates: readonly CandidateView[]): Selected {
  const checked = candidates.filter((candidate) => isChecked(selection, candidate));
  const readyOf = (checks: Iterable<AppCheck>) =>
    [...checks].flatMap((check) => (check.kind === "ready" ? [check] : []));
  const ready = readyOf(selection.apps.values());
  const copies = readyOf([...selection.copies.values()].map((copy) => copy.check));
  return {
    count: checked.length + ready.length + copies.length,
    items: checked.length,
    bytes: sumBytes([
      ...checked.map((candidate) => candidate.reclaimable_bytes),
      ...ready.map((app) => app.bytes),
      ...copies.map((copy) => copy.bytes),
    ]),
    apps: ready.map((app) => ({ preview: app.preview, confirmation: app.hash })),
    copies: copies.map((copy) => ({ preview: copy.preview, confirmation: copy.hash })),
    chosen: checked.filter((candidate) => !proposed(candidate)).map((candidate) => candidate.id),
    leftOut: candidates
      .filter((candidate) => proposed(candidate) && selection.leftOut.has(candidate.id))
      .map((candidate) => candidate.id),
  };
}
