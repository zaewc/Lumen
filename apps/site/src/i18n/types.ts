// The shape of the site's copy. ko.ts and en.ts both have this type, so the
// two languages cannot drift apart: a missing or extra key fails `tsc`.

import type { PlatformId } from "../data/release";
import type { ClientStrings } from "./client";

export type Locale = "ko" | "en";

/** A run of text with a little inline markup, rendered by Rich.astro. */
export type Inline =
  | string
  | { readonly code: string }
  | { readonly ui: string }
  | { readonly link: string; readonly href: string };

export type Rich = readonly Inline[];

export interface Messages {
  readonly lang: Locale;
  /** The language link's text, in the other language. */
  readonly otherLanguage: string;
  readonly description: string;
  readonly ogDescription: string;
  readonly skip: string;
  readonly nav: {
    readonly label: string;
    readonly docs: string;
  };
  readonly hero: {
    readonly headline: readonly [string, string];
    readonly lead: string;
    readonly download: string;
    readonly requirements: string;
  };
  readonly trust: {
    readonly items: readonly {
      readonly headline: readonly [string, string];
      readonly text: string;
    }[];
    readonly more: string;
  };
  readonly download: {
    readonly title: string;
    readonly noScript: Rich;
    readonly checking: string;
    readonly thisComputer: string;
    readonly installer: string;
    readonly requirements: Readonly<Record<PlatformId, string>>;
  };
  readonly files: { readonly summary: string; readonly intro: Rich };
  readonly verify: {
    readonly summary: string;
    /** Text around the SHA256SUMS link. */
    readonly introBefore: Rich;
    readonly introAfter: Rich;
    readonly windowsNote: string;
    readonly provenance: string;
    readonly provenanceNote: Rich;
  };
  readonly install: {
    readonly title: string;
    readonly intro: string;
    readonly mac: { readonly steps: readonly Rich[]; readonly aside: Rich };
    readonly fda: {
      readonly summary: string;
      readonly intro: string;
      readonly steps: readonly Rich[];
      readonly aside: Rich;
    };
    readonly windows: { readonly steps: readonly Rich[] };
    readonly linux: { readonly aside: Rich };
  };
  readonly footer: {
    readonly source: string;
    readonly releases: string;
  };
  readonly copy: string;
  readonly client: ClientStrings;
  readonly notFound: { readonly title: string; readonly home: string };
}
