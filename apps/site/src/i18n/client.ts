// The few strings the release lookup writes into the page. Astro renders them
// into <body data-strings> for the page's language; the script reads them back
// with parseClientStrings, so no inline script is needed.

import { z } from "../lib/zod";

export const CLIENT_KEYS = [
  "dateLocale",
  "download",
  "downloadFor",
  "viewOnGitHub",
  "downloadFromGitHub",
  "getFromGitHub",
  "comingSoonShort",
  "loadFailed",
  "checkFailed",
  "rateLimited",
  "releaseNotes",
  "githubReleases",
  "allFiles",
  "fileHeader",
  "sizeHeader",
  "copied",
] as const;

export type ClientKey = (typeof CLIENT_KEYS)[number];

/** `downloadFor` holds `{platform}`, replaced by the platform's name. */
export type ClientStrings = Readonly<Record<ClientKey, string>>;

const clientStrings = z
  .record(z.enum(CLIENT_KEYS), z.string())
  .refine((strings) => strings.dateLocale === "ko-KR" || strings.dateLocale === "en-US");

export function parseClientStrings(raw: string | undefined): ClientStrings | null {
  if (raw === undefined) {
    return null;
  }
  try {
    const json: unknown = JSON.parse(raw);
    const parsed = clientStrings.safeParse(json);
    return parsed.success ? parsed.data : null;
  } catch {
    return null;
  }
}
